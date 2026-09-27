# A13 (EXPLORATORY): partial receipt visibility, α = 0.2

Source: `e1-visibility.csv` (20 seeds). Steady-state attacker share, mean [95% CI].
v is the probability that an observer sees a given other honest requester's receipts;
Sybil receipts stay fully visible. Random selection gives 0.2.

## farming

| v | B1 | Ours-L W0 | Ours-L W1 | Ours-L+C W1 | W1 reading |
|---|---|---|---|---|---|
| 1 | 0.097 [0.093, 0.101] | 0.072 [0.071, 0.073] | 0.027 [0.027, 0.028] | 0.002 [0.002, 0.002] | keeps its advantage |
| 0.5 | 0.143 [0.139, 0.147] | 0.073 [0.072, 0.074] | 0.029 [0.029, 0.029] | 0.003 [0.002, 0.003] | keeps its advantage |
| 0.2 | 0.290 [0.282, 0.297] | 0.072 [0.072, 0.073] | 0.033 [0.033, 0.034] | 0.003 [0.003, 0.003] | keeps its advantage |
| 0.05 | 0.678 [0.675, 0.681] | 0.072 [0.071, 0.072] | 0.044 [0.044, 0.045] | 0.004 [0.004, 0.004] | keeps its advantage |

## camouflage

| v | B1 | Ours-L W0 | Ours-L W1 | Ours-L+C W1 | W1 reading |
|---|---|---|---|---|---|
| 1 | 0.087 [0.084, 0.090] | 0.073 [0.072, 0.074] | 0.041 [0.040, 0.042] | 0.008 [0.008, 0.008] | keeps its advantage |
| 0.5 | 0.147 [0.143, 0.152] | 0.072 [0.072, 0.073] | 0.063 [0.062, 0.064] | 0.010 [0.010, 0.010] | keeps its advantage |
| 0.2 | 0.264 [0.258, 0.270] | 0.073 [0.072, 0.074] | 0.103 [0.101, 0.104] | 0.012 [0.012, 0.012] | loses to self-only |
| 0.05 | 0.544 [0.536, 0.554] | 0.073 [0.072, 0.073] | 0.146 [0.144, 0.148] | 0.014 [0.014, 0.015] | loses to self-only |

## slander

| v | B1 | Ours-L W0 | Ours-L W1 | Ours-L+C W1 | W1 reading |
|---|---|---|---|---|---|
| 1 | 0.402 [0.396, 0.407] | 0.073 [0.072, 0.073] | 0.027 [0.027, 0.027] | 0.002 [0.002, 0.002] | keeps its advantage |
| 0.5 | 0.647 [0.643, 0.651] | 0.072 [0.071, 0.073] | 0.029 [0.029, 0.030] | 0.003 [0.002, 0.003] | keeps its advantage |
| 0.2 | 0.714 [0.713, 0.714] | 0.072 [0.071, 0.073] | 0.034 [0.034, 0.035] | 0.003 [0.003, 0.003] | keeps its advantage |
| 0.05 | 0.714 [0.713, 0.715] | 0.072 [0.072, 0.073] | 0.044 [0.044, 0.045] | 0.004 [0.004, 0.004] | keeps its advantage |

## Honest success (steady state), Ours-L W1

| attack | v = 1 | v = 0.5 | v = 0.2 | v = 0.05 |
|---|---|---|---|---|
| farming | 0.998 [0.997, 0.998] | 0.997 [0.997, 0.997] | 0.997 [0.997, 0.997] | 0.996 [0.996, 0.996] |
| camouflage | 0.997 [0.997, 0.997] | 0.997 [0.997, 0.997] | 0.996 [0.995, 0.996] | 0.993 [0.993, 0.993] |
| slander | 0.998 [0.997, 0.998] | 0.997 [0.997, 0.997] | 0.997 [0.997, 0.997] | 0.996 [0.995, 0.996] |

## Readings (rules fixed in A13)

- farming, v = 1: keeps its advantage
- farming, v = 0.5: keeps its advantage
- farming, v = 0.2: keeps its advantage
- farming, v = 0.05: keeps its advantage
- camouflage, v = 1: keeps its advantage
- camouflage, v = 0.5: keeps its advantage
- camouflage, v = 0.2: loses to self-only
- camouflage, v = 0.05: loses to self-only
- slander, v = 1: keeps its advantage
- slander, v = 0.5: keeps its advantage
- slander, v = 0.2: keeps its advantage
- slander, v = 0.05: keeps its advantage

