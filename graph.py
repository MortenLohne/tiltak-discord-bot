import matplotlib.pyplot as plt
import matplotlib.ticker as mticker
import numpy as np
import re
import sys

BACKGROUND = "#404040"
EVALUATION = "#fb8b24"
WIDTH_PER_PLY = 0.2

evals = np.array(
    [
        (float(match) / 100)
        for match in re.findall("(\d{1,3}\.\d)%", sys.stdin.read())
    ]
)
plies = evals.size

# plotting
fig = plt.figure(figsize=(WIDTH_PER_PLY * plies, 5), tight_layout=True, dpi=200)

ax = plt.axes()
ax.set_facecolor(BACKGROUND)

less = evals < 0.5
black = less | np.roll(less, 1)
white = ~less | np.roll(~less, 1)
b_evals = evals.clip(max=0.5)
w_evals = evals.clip(min=0.5)

x = 1 + np.arange(plies) / 2
middle = np.full(plies, 0.5)

ax.plot(x, middle, color="gray")
ax.plot(x, evals, drawstyle="steps-post", color=EVALUATION)
ax.fill_between(x, b_evals, middle, step="post", where=black, color="black")
ax.fill_between(x, w_evals, middle, step="post", where=white, color="white")

ax.set_title("Evaluation Graph")
ax.set_xlabel("Move Number")
ax.set_ylabel("Evaluation")

ax.set_xbound(1, (plies + 1) / 2)
ax.set_ybound(0, 1)
ax.set_xticks(x[::2])
ax.yaxis.set_major_formatter(mticker.PercentFormatter(xmax=1.0, decimals=0))

plt.savefig(sys.stdout.buffer)
