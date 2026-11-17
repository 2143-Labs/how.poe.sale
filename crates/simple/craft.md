# Supreme Spiked Shield (shaper), item level 87

- prefix pool: 73 modifiers, total weight 64800
- suffix pool: 131 modifiers, total weight 96475
- one desired prefix: 1/81 = 1.2346%
- one good prefix: 5/81 = 6.1728%
- one good suffix: 584/3859 = 15.1335%
- target: a rare item, prefixes [desired, good, any], suffixes [good, good, any]

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
  s0["alt_aug_regal<br/>normal, magic → rare<br/>past this step: 34.5 ± 34.1 chaos"]
  s1["exalt_to(4)<br/>rare → rare, normal<br/>past this step: 127.4 ± 125.0 chaos"]
  s2["exalt_to(5)<br/>rare → rare, normal<br/>past this step: 517.5 ± 513.2 chaos"]
  s3["exalt_to(6)<br/>rare → rare, normal<br/>past this step: 3399.5 ± 3393.3 chaos"]
  target(["target<br/>3399.5 ± 3393.3 chaos"])
  s0 -->|"rare 100.00%"| s1
  s1 -->|"slammed 28.95%"| s2
  s1 -.->|"scrap 71.05%"| s0
  s2 -->|"slammed 25.07%"| s3
  s2 -.->|"scrap 74.93%"| s0
  s3 -->|"slammed 15.29%"| target
  s3 -.->|"scrap 84.71%"| s0
```

| step | method | takes | chance | tries | one run costs | cost to get past it | chaos ± sd |
|---|---|---|---|---|---|---|---|
| 0 | alt_aug_regal | normal, magic | 100.00% | 1.00 | alteration 167.01, augmentation 0.52, regal 1.56, scouring 0.56, transmutation 1.56 | alteration 167.01, augmentation 0.52, regal 1.56, scouring 0.56, transmutation 1.56 | 34.5 ± 34.1 |
| 1 | exalt_to(4) | rare | 28.95% | 3.45 | exalted 1.00, scouring 0.71 | alteration 576.81, augmentation 1.80, exalted 3.45, regal 5.39, scouring 4.39, transmutation 5.39 | 127.4 ± 125.0 |
| 2 | exalt_to(5) | rare | 25.07% | 3.99 | exalted 1.00, scouring 0.75 | alteration 2300.85, augmentation 7.17, exalted 17.77, regal 21.50, scouring 20.50, transmutation 21.50 | 517.5 ± 513.2 |
| 3 | exalt_to(6) | rare | 15.29% | 6.54 | exalted 1.00, scouring 0.85 | alteration 15044.32, augmentation 46.87, exalted 122.70, regal 140.60, scouring 139.60, transmutation 140.60 | 3399.5 ± 3393.3 |

Whole craft, per orb: alteration 15044.32 ± 15044.82, augmentation 46.87 ± 47.03, exalted 122.70 ± 120.50, regal 140.60 ± 140.10, scouring 139.60 ± 140.10, transmutation 140.60 ± 140.10

## Method: alt_aug_regal (step 0)

```mermaid
flowchart TD
  b0["normal<br/>transmutation"]
  b1["magic desired / -<br/>augmentation"]
  b2["magic (any grades)<br/>alteration"]
  b3["magic desired / good<br/>regal"]
  b4["magic desired / any<br/>regal"]
  b5(["exit rare<br/>to step 1"])
  b6["rare (any grades)<br/>scouring"]
  b0 -->|"0.31%"| b1
  b0 -->|"99.07%"| b2
  b0 -->|"0.09%"| b3
  b0 -->|"0.52%"| b4
  b1 -->|"15.13%"| b3
  b1 -->|"84.87%"| b4
  b2 -->|"0.31%"| b1
  b2 -->|"99.07%"| b2
  b2 -->|"0.09%"| b3
  b2 -->|"0.52%"| b4
  b3 -->|"100.00%"| b5
  b4 -->|"57.66%"| b5
  b4 -->|"42.34%"| b6
  b6 -->|"100.00%"| b0
```

## Method: exalt_to(4) (step 1)

```mermaid
flowchart TD
  b0(["exit scrap<br/>to step 0"])
  b1["rare desired+good / good<br/>exalted"]
  b2(["exit slammed<br/>to step 2"])
  b3["rare desired+any / good<br/>exalted"]
  b4["rare (any grades)<br/>scouring"]
  b5["rare desired / good+good<br/>exalted"]
  b6["rare desired / good+any<br/>exalted"]
  b7["rare desired+good / any<br/>exalted"]
  b8["rare desired+any / any<br/>exalted"]
  b1 -->|"100.00%"| b2
  b3 -->|"53.07%"| b2
  b3 -->|"46.93%"| b4
  b5 -->|"100.00%"| b2
  b6 -->|"57.24%"| b2
  b6 -->|"42.76%"| b4
  b7 -->|"57.66%"| b2
  b7 -->|"42.34%"| b4
  b8 -->|"10.73%"| b2
  b8 -->|"89.27%"| b4
  b4 -->|"100.00%"| b0
```

## Method: exalt_to(5) (step 2)

```mermaid
flowchart TD
  b0(["exit scrap<br/>to step 0"])
  b1["rare desired+good+good / good<br/>exalted"]
  b2(["exit slammed<br/>to step 3"])
  b3["rare desired+good+any / good<br/>exalted"]
  b4["rare desired+good / good+good<br/>exalted"]
  b5["rare desired+good / good+any<br/>exalted"]
  b6["rare (any grades)<br/>scouring"]
  b7["rare desired+any / good+good<br/>exalted"]
  b8["rare desired+any / good+any<br/>exalted"]
  b9["rare desired / good+good+good<br/>exalted"]
  b10["rare desired / good+good+any<br/>exalted"]
  b11["rare desired+good+good / any<br/>exalted"]
  b12["rare desired+good+any / any<br/>exalted"]
  b1 -->|"100.00%"| b2
  b3 -->|"100.00%"| b2
  b4 -->|"100.00%"| b2
  b5 -->|"57.24%"| b2
  b5 -->|"42.76%"| b6
  b7 -->|"53.07%"| b2
  b7 -->|"46.93%"| b6
  b8 -->|"10.31%"| b2
  b8 -->|"89.69%"| b6
  b9 -->|"100.00%"| b2
  b10 -->|"100.00%"| b2
  b11 -->|"15.32%"| b2
  b11 -->|"84.68%"| b6
  b12 -->|"15.32%"| b2
  b12 -->|"84.68%"| b6
  b6 -->|"100.00%"| b0
```

## Method: exalt_to(6) (step 3)

```mermaid
flowchart TD
  b0(["exit scrap<br/>to step 0"])
  b1["rare desired+good+good / good+good<br/>exalted"]
  b2(["exit slammed<br/>to done"])
  b3["rare desired+good+good / good+any<br/>exalted"]
  b4["rare (any grades)<br/>scouring"]
  b5["rare desired+good+any / good+good<br/>exalted"]
  b6["rare desired+good+any / good+any<br/>exalted"]
  b7["rare desired+good / good+good+good<br/>exalted"]
  b8["rare desired+good / good+good+any<br/>exalted"]
  b9["rare desired+any / good+good+good<br/>exalted"]
  b10["rare desired+any / good+good+any<br/>exalted"]
  b1 -->|"100.00%"| b2
  b3 -->|"14.48%"| b2
  b3 -->|"85.52%"| b4
  b5 -->|"100.00%"| b2
  b6 -->|"14.48%"| b2
  b6 -->|"85.52%"| b4
  b7 -->|"100.00%"| b2
  b8 -->|"100.00%"| b2
  b9 -->|"6.14%"| b2
  b9 -->|"93.86%"| b4
  b10 -->|"6.14%"| b2
  b10 -->|"93.86%"| b4
  b4 -->|"100.00%"| b0
```

## Other moves

84 other moves checked at 68 places the craft passes through; 6 cost less to finish, by the weights.

| step | item | visits | planned | to finish (chaos) | instead | to finish (chaos) |
|---|---|---|---|---|---|---|
| 0 | magic good / good | 70.93 | alteration | 3399.70 | regal | 3375.84 |
| 0 | magic - / good | 574.50 | alteration | 3399.70 | augmentation | 3398.46 |
| 0 | magic good / - | 234.34 | alteration | 3399.70 | regal | 3397.74 |
| 0 | magic - / good | 574.50 | alteration | 3399.70 | regal | 3399.50 |
| 0 | magic - / any | 3221.73 | alteration | 3399.70 | augmentation | 3399.67 |
| 0 | magic good / any | 397.74 | alteration | 3399.70 | regal | 3399.54 |

