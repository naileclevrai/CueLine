"""Generates the CueLine icon (ICO for the executable, raw RGBA for the window).

Run from the repository root:  python assets/make_icon.py
"""
from PIL import Image, ImageDraw

S = 1024  # supersampled master


def master():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([40, 40, S - 40, S - 40], radius=200, fill=(26, 27, 31, 255), outline=(58, 61, 69, 255), width=16)
    # Biphase-mark LTC trace in amber.
    amber = (240, 168, 48, 255)
    hi, lo = 380, 640
    x, level = 176, True
    pattern = [1, 1, 2, 1, 1, 2, 2, 1, 1]
    pts = []
    for p in pattern:
        y = hi if level else lo
        pts.append((x, y))
        x += 56 * p
        pts.append((x, y))
        level = not level
    d.line(pts, fill=amber, width=46, joint="curve")
    # Playhead.
    d.rectangle([492, 210, 532, 820], fill=(255, 85, 74, 255))
    d.polygon([(452, 200), (572, 200), (512, 290)], fill=(255, 85, 74, 255))
    return img


m = master()
sizes = [16, 24, 32, 48, 64, 128, 256]
m.resize((256, 256), Image.LANCZOS).save("assets/cueline.ico", sizes=[(s, s) for s in sizes])
m.resize((512, 512), Image.LANCZOS).save("assets/cueline.png")
rgba = m.resize((64, 64), Image.LANCZOS).tobytes()
open("assets/icon-64.rgba", "wb").write(rgba)
print("icon written")
