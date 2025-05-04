"""Regenerate original FileOrganizer artwork. Developer-only dependency: Pillow."""
from pathlib import Path

from PIL import Image, ImageDraw


def draw_icon(size: int) -> Image.Image:
    scale = 4
    image = Image.new("RGBA", (size * scale, size * scale))
    draw = ImageDraw.Draw(image)
    ratio = size * scale / 256

    def rect(box, radius, fill):
        draw.rounded_rectangle(tuple(round(v * ratio) for v in box), round(radius * ratio), fill=fill)

    # A single folder with three filing dividers stays readable at title-bar size.
    rect((12, 12, 244, 244), 52, "#146B61")
    rect((42, 69, 114, 112), 12, "#AED8C9")
    rect((42, 89, 214, 185), 12, "#AED8C9")
    for left, top in [(67, 100), (108, 91), (149, 100)]:
        rect((left, top, left + 31, 156), 5, "#FFFFFF")
    rect((38, 120, 218, 199), 14, "#F8F5E8")
    for left in (69, 111, 153):
        rect((left, 145, left + 26, 172), 4, "#146B61")
    return image.resize((size, size), Image.Resampling.LANCZOS)


if __name__ == "__main__":
    assets = Path(__file__).resolve().parents[1] / "assets"
    assets.mkdir(exist_ok=True)
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    images = [draw_icon(size) for size in sizes]
    images[-1].save(assets / "icon-256.png")
    images[-1].save(assets / "FileOrganizer.ico", sizes=[(s, s) for s in sizes], append_images=images[:-1])
