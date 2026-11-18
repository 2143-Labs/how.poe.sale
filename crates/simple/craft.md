# Supreme Spiked Shield (shaper), item level 87

- prefix pool: 73 modifiers, total weight 64800
- suffix pool: 131 modifiers, total weight 96475
- one desired prefix: 1/81 = 1.2346%
- one good prefix: 5/81 = 6.1728%
- one good suffix: 584/3859 = 15.1335%
- target: a rare item, prefixes [desired], suffixes []

The strategy is written down and this says what it costs. A chance is used one step deep; what is carried from step to step is the cost to get there and its spread.

## Currencies

| currency | works on | one use costs | weight (chaos) |
|---|---|---|---|
| transmutation | normal | 1 transmutation | 0.02 |
| alteration | magic | 1 alteration | 0.2 |
| augmentation | magic | 1 augmentation | 0.05 |
| regal | magic | 1 regal | 0.5 |
| exalted | rare | 1 exalted | 2 |
| scouring | magic, rare | 1 scouring | 0.5 |

## Craft: shield (forward)

```mermaid
flowchart LR
  s0["alt_aug_regal<br/>normal, magic → rare<br/>past this step: 17.5 ± 17.1 chaos"]
  target(["target<br/>17.5 ± 17.1 chaos"])
  s0 -->|"rare 100.00%"| target
```

| step | method | takes | chance | tries | one run costs | cost to get past it | chaos ± sd |
|---|---|---|---|---|---|---|---|
| 0 | alt_aug_regal | normal, magic | 100.00% | 1.00 | alteration 80.00, augmentation 20.50, regal 1.00, transmutation 1.00 | alteration 80.00, augmentation 20.50, regal 1.00, transmutation 1.00 | 17.5 ± 17.1 |

Whole craft, per orb: alteration 80.00 ± 80.50, augmentation 20.50 ± 20.50, regal 1.00 ± 0.00, transmutation 1.00 ± 0.00

## Method: alt_aug_regal (step 0)

```mermaid
flowchart TD
  b0["normal<br/>transmutation"]
  b1["magic desired / -<br/>augmentation"]
  b2["magic (any grades)<br/>alteration"]
  b3["magic - / good<br/>augmentation"]
  b4["magic - / any<br/>augmentation"]
  b5["magic desired / good<br/>regal"]
  b6["magic desired / any<br/>regal"]
  b7(["exit rare<br/>to done"])
  b0 -->|"0.31%"| b1
  b0 -->|"74.07%"| b2
  b0 -->|"3.78%"| b3
  b0 -->|"21.22%"| b4
  b0 -->|"0.09%"| b5
  b0 -->|"0.52%"| b6
  b1 -->|"15.13%"| b5
  b1 -->|"84.87%"| b6
  b2 -->|"0.31%"| b1
  b2 -->|"74.07%"| b2
  b2 -->|"3.78%"| b3
  b2 -->|"21.22%"| b4
  b2 -->|"0.09%"| b5
  b2 -->|"0.52%"| b6
  b3 -->|"1.23%"| b5
  b3 -->|"98.77%"| b2
  b4 -->|"1.23%"| b6
  b4 -->|"98.77%"| b2
  b5 -->|"100.00%"| b7
  b6 -->|"100.00%"| b7
```

## Other moves

27 other moves checked at 11 places the craft passes through; 1 cost less to finish, by the weights.

| step | item | visits | planned | to finish (chaos) | instead | to finish (chaos) |
|---|---|---|---|---|---|---|
| 0 | magic desired / - | 0.25 | augmentation | 0.55 | regal | 0.50 |

