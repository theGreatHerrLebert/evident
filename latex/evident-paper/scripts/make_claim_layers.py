from pathlib import Path

import matplotlib.pyplot as plt
from matplotlib.patches import FancyBboxPatch


OUT = Path(__file__).resolve().parents[1] / "figures" / "claim_layers.pdf"


def add_box(ax, xy, width, height, title, body, facecolor, edgecolor):
    box = FancyBboxPatch(
        xy,
        width,
        height,
        boxstyle="round,pad=0.03,rounding_size=0.035",
        linewidth=1.25,
        facecolor=facecolor,
        edgecolor=edgecolor,
    )
    ax.add_patch(box)
    x, y = xy
    ax.text(
        x + 0.05,
        y + height - 0.18,
        title,
        ha="left",
        va="top",
        fontsize=11,
        fontweight="bold",
        color="#172033",
    )
    ax.text(
        x + 0.05,
        y + height - 0.41,
        body,
        ha="left",
        va="top",
        fontsize=8.4,
        linespacing=1.25,
        color="#263247",
    )


def add_arrow(ax, y0, y1):
    ax.annotate(
        "",
        xy=(2.85, y1),
        xytext=(2.85, y0),
        arrowprops=dict(arrowstyle="-|>", lw=1.25, color="#5d667a"),
    )
    ax.text(
        3.45,
        (y0 + y1) / 2,
        "not sufficient",
        ha="left",
        va="center",
        fontsize=8.2,
        color="#9a3412",
        bbox=dict(boxstyle="round,pad=0.18", facecolor="white", edgecolor="none"),
    )


def main():
    fig, ax = plt.subplots(figsize=(6.9, 4.3))
    ax.set_xlim(0, 5.7)
    ax.set_ylim(0, 4.3)
    ax.axis("off")

    add_box(
        ax,
        (0.575, 3.15),
        4.55,
        0.75,
        "Implementation claim",
        "A component behaves according to a local specification.\nExample: parser preserves coordinates.",
        "#e8f2ff",
        "#2f6fae",
    )
    add_box(
        ax,
        (0.575, 1.85),
        4.55,
        0.75,
        "Pipeline claim",
        "A workflow transforms inputs into outputs reproducibly.\nExample: report table is reproduced.",
        "#eaf7ef",
        "#2f7d54",
    )
    add_box(
        ax,
        (0.575, 0.55),
        4.55,
        0.75,
        "Scientific claim",
        "Outputs support interpretation under stated assumptions.\nExample: result supports a conclusion.",
        "#fff1e7",
        "#b65f1f",
    )

    add_arrow(ax, 2.97, 2.63)
    add_arrow(ax, 1.67, 1.33)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(OUT, bbox_inches="tight")


if __name__ == "__main__":
    main()
