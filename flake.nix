{
  description = "how.poe.sale - Path of Exile crafting solver";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, naersk, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; overlays = [ rust-overlay.overlays.default ]; };
        rust-toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        # Packages build with a minimal toolchain of the same channel. With rust-src installed, rustc
        # rewrites std's /rustc/<hash> panic paths to the local source copy, and that one string pins the
        # whole dev toolchain (~600 MB) into the server binary's runtime closure.
        rustChannel = (builtins.fromTOML (builtins.readFile ./rust-toolchain.toml)).toolchain.channel;
        build-toolchain = pkgs.rust-bin.stable.${rustChannel}.minimal.override { targets = [ "wasm32-unknown-unknown" ]; };
        naersk-lib = pkgs.callPackage naersk { cargo = build-toolchain; rustc = build-toolchain; };

        # repoe-fork export of 3.29.3.3; keep in sync with scripts/fetch-data.sh.
        repoeCommit = "d8635430f4005faccfed69c7957ef2fb9f8fb1fb";
        repoeFiles = [
          { name = "mods"; hash = "sha256-3vu6MbYPkZEIDHgZcngFf4NPf5J8GioICnym7T6mhrU="; }
          { name = "base_items"; hash = "sha256-SK+wel9A61FVL5DBfVMYZGnbKCoudZ2re0bn+dKg9Ys="; }
          { name = "item_classes"; hash = "sha256-Z4+jumU8J1DDjduk7tLA99bF7AQLK7wHvFAv7WnF5jI="; }
          { name = "stat_translations"; hash = "sha256-PmIQZ4Lu6e5t1o8Ed65TdSIgCXACCLmgsTQ1ooLTvvo="; }
          { name = "crafting_bench_options"; hash = "sha256-D+UasWV86mnFM7sGEOOT8QQBFWFA3ojt9oYSBIPNKEU="; }
          { name = "tags"; hash = "sha256-Jd+ROr1Fm+3T3BtknjRLNT4nHtCjIWLxFtZgNKMmm1A="; }
        ];
        repoeRaw = pkgs.linkFarm "repoe-raw-3.29.3.3" (map
          (f: {
            name = "${f.name}.json";
            path = pkgs.fetchurl {
              url = "https://raw.githubusercontent.com/repoe-fork/repoe-fork.github.io/${repoeCommit}/data/${f.name}.json";
              inherit (f) hash;
            };
          })
          repoeFiles);

        crate = name: naersk-lib.buildPackage {
          name = "how-poe-sale-${name}";
          version = "0.1.0";
          src = ./.;
          cargoBuildOptions = x: x ++ [ "-p" name ];
        };
        datagen = crate "datagen";
        server = crate "server";

        bundle = pkgs.runCommand "how-poe-sale-bundle" { } ''
          mkdir -p $out
          ${datagen}/bin/datagen --raw ${repoeRaw} --out $out/bundle.json
        '';

        wasmRaw = naersk-lib.buildPackage {
          name = "how-poe-sale-wasm-api";
          version = "0.1.0";
          src = ./.;
          copyLibs = true;
          CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
          cargoBuildOptions = x: x ++ [ "-p" "wasm-api" ];
        };
        wasm = pkgs.stdenv.mkDerivation {
          name = "how-poe-sale-wasm";
          nativeBuildInputs = [ pkgs.wasm-bindgen-cli_0_2_129 pkgs.binaryen ];
          dontUnpack = true;
          buildPhase = ''
            runHook preBuild
            wasm=$(echo ${wasmRaw}/lib/*.wasm)
            wasm-opt -Os --output opt.wasm "$wasm"
            wasm-bindgen --target web --out-name solver --out-dir $out opt.wasm
            runHook postBuild
          '';
          dontInstall = true;
        };

        frontend = pkgs.buildNpmPackage {
          pname = "how-poe-sale-web";
          version = "0.1.0";
          src = ./web;
          nodejs = pkgs.nodejs_24;
          npmDeps = pkgs.importNpmLock { npmRoot = ./web; };
          npmConfigHook = pkgs.importNpmLock.npmConfigHook;
          WASM_PKG = "${wasm}";
          BUNDLE = "${bundle}/bundle.json";
          installPhase = ''
            runHook preInstall
            mkdir -p $out
            cp -r dist/. $out/
            runHook postInstall
          '';
        };

        webRoot = pkgs.runCommand "how-poe-sale-webroot" { } ''
          mkdir -p $out/web
          cp -r ${frontend}/. $out/web/
        '';

        container = pkgs.dockerTools.buildLayeredImage {
          name = "how-poe-sale";
          tag = "latest";
          contents = [ server webRoot pkgs.cacert ];
          config = {
            ExposedPorts = { "8080/tcp" = { }; };
            Entrypoint = [ "${server}/bin/server" ];
            Env = [ "RUST_LOG=info" "STATIC_DIR=/web" "PORT=8080" ];
            Labels = { "org.opencontainers.image.source" = "https://github.com/2143-Labs/how.poe.sale"; };
          };
        };
      in
      {
        packages = {
          inherit datagen server bundle wasmRaw wasm frontend webRoot container;
          default = container;
        };

        # Tools only: entering the shell must never build the wasm or the bundle.
        devShells.default = pkgs.mkShell {
          packages = [
            rust-toolchain
            pkgs.nodejs_24
            pkgs.wasm-bindgen-cli_0_2_129
            pkgs.binaryen
            pkgs.pkg-config
            pkgs.jq
            pkgs.curl
            pkgs.python3
          ];
        };
      });
}
