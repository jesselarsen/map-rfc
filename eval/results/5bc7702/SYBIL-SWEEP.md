# A10 (EXPLORATORY): Sybil-count sweep, α = 0.2

Source: `e1-sybil-sweep.csv` (arm `ours`, 20 seeds). Steady-state attacker share
and honest success, mean [95% CI]. Random selection gives α = 0.2.

## camouflage

| Sybils per adv. provider | Sybils : honest requesters | camouflage tasks bought per round (× honest workload) | w0 share | w1 share | w2 share | w0 honest success | w1 honest success | w2 honest success |
|---|---|---|---|---|---|---|---|---|
| 5 | 200 : 500 (0.4) | 400 (0.4×) | 0.073 [0.072, 0.074] | 0.031 [0.030, 0.031] | 0.027 [0.027, 0.028] | 0.993 [0.992, 0.993] | 0.998 [0.997, 0.998] | 0.998 [0.998, 0.998] |
| 20 | 800 : 500 (1.6) | 1600 (1.6×) | 0.072 [0.071, 0.073] | 0.041 [0.040, 0.041] | 0.045 [0.044, 0.046] | 0.993 [0.993, 0.993] | 0.997 [0.997, 0.997] | 0.997 [0.997, 0.997] |
| 80 | 3200 : 500 (6.4) | 6400 (6.4×) | 0.072 [0.071, 0.072] | 0.077 [0.075, 0.079] | 0.196 [0.193, 0.200] | 0.993 [0.993, 0.993] | 0.997 [0.996, 0.997] | 0.981 [0.980, 0.982] |
| 320 | 12800 : 500 (25.6) | 25600 (25.6×) | 0.072 [0.071, 0.073] | 0.165 [0.162, 0.168] | 0.526 [0.522, 0.530] | 0.993 [0.993, 0.994] | 0.992 [0.992, 0.993] | 0.917 [0.916, 0.918] |

Readings (camouflage, rules fixed in A10):

- w1, s = 5: none of the three
- w1, s = 20: none of the three
- w1, s = 80: degrades vs s = 20, loses to self-only (W0)
- w1, s = 320: degrades vs s = 20, loses to self-only (W0)
- w2, s = 5: none of the three
- w2, s = 20: none of the three
- w2, s = 80: degrades vs s = 20, loses to self-only (W0)
- w2, s = 320: degrades vs s = 20, loses to self-only (W0), worse than random

## farming

| Sybils per adv. provider | Sybils : honest requesters | w0 share | w1 share | w2 share | w0 honest success | w1 honest success | w2 honest success |
|---|---|---|---|---|---|---|---|
| 5 | 200 : 500 (0.4) | 0.072 [0.071, 0.073] | 0.027 [0.026, 0.027] | 0.025 [0.025, 0.026] | 0.993 [0.993, 0.993] | 0.998 [0.998, 0.998] | 0.998 [0.998, 0.998] |
| 20 | 800 : 500 (1.6) | 0.073 [0.072, 0.074] | 0.027 [0.027, 0.028] | 0.025 [0.025, 0.026] | 0.993 [0.993, 0.993] | 0.998 [0.997, 0.998] | 0.998 [0.998, 0.998] |
| 80 | 3200 : 500 (6.4) | 0.072 [0.071, 0.073] | 0.027 [0.027, 0.027] | 0.025 [0.025, 0.026] | 0.993 [0.993, 0.993] | 0.998 [0.998, 0.998] | 0.998 [0.998, 0.998] |
| 320 | 12800 : 500 (25.6) | 0.072 [0.072, 0.073] | 0.027 [0.026, 0.027] | 0.026 [0.025, 0.026] | 0.993 [0.993, 0.993] | 0.998 [0.997, 0.998] | 0.998 [0.998, 0.998] |

Readings (farming, rules fixed in A10):

- w1, s = 5: none of the three
- w1, s = 20: none of the three
- w1, s = 80: none of the three
- w1, s = 320: none of the three
- w2, s = 5: none of the three
- w2, s = 20: none of the three
- w2, s = 80: none of the three
- w2, s = 320: none of the three

