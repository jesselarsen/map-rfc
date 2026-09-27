# A12 (EXPLORATORY): camouflaged targeted slander, α = 0.2, 5 targets

Source: `e1-targeted-slander.csv` (scenario `e1t`, 20 seeds). Mean [95% CI].
Target share is the share of honest requesters' executions that go to the 5 targets
(rounds 31–60). Relative harm = 1 − share(attack) / share(none).

| arm | camouflage tasks | target share, none | target share, attack | relative harm | attacker share, attack | reading |
|---|---|---|---|---|---|---|
| B1 global | 0 | 0.044 [0.030, 0.058] | 0.003 [0.003, 0.003] | 0.93 | 0.024 [0.024, 0.025] | harms targets |
| B1 global | 2 | 0.035 [0.025, 0.046] | 0.003 [0.003, 0.003] | 0.91 | 0.024 [0.024, 0.025] | harms targets |
| Ours-L W0 | 0 | 0.029 [0.028, 0.030] | 0.029 [0.028, 0.030] | 0.01 | 0.072 [0.071, 0.073] | no harm |
| Ours-L W0 | 2 | 0.029 [0.028, 0.030] | 0.029 [0.028, 0.030] | 0.01 | 0.072 [0.071, 0.073] | no harm |
| Ours-L W1 | 0 | 0.028 [0.023, 0.033] | 0.032 [0.028, 0.037] | -0.17 | 0.027 [0.026, 0.027] | no harm |
| Ours-L W1 | 2 | 0.035 [0.031, 0.039] | 0.011 [0.011, 0.012] | 0.68 | 0.027 [0.026, 0.027] | harms targets |
| Ours-L W2 | 0 | 0.028 [0.022, 0.034] | 0.027 [0.023, 0.032] | 0.03 | 0.025 [0.025, 0.026] | no harm |
| Ours-L W2 | 2 | 0.031 [0.024, 0.038] | 0.004 [0.004, 0.004] | 0.88 | 0.026 [0.025, 0.026] | harms targets |

Camouflage worsens the attack (camouflage 2 CI entirely below camouflage 0 CI):

- B1 global: no
- Ours-L W0: no
- Ours-L W1: yes
- Ours-L W2: yes

