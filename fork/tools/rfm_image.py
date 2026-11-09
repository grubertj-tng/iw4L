"""The one image transform between a captured game frame and a model input.

Dataset conversion and inference must feed the model identically prepared
images, so both import this module and nothing else resizes frames.

A frame is RGB uint8 (H, W, 3) at the capture size (the game window, 16:9).
"""

from dataclasses import dataclass

import numpy as np
from PIL import Image


@dataclass(frozen=True)
class ImageSpec:
    width: int = 256
    height: int = 144
    # "fit": keep the aspect and pad (letterbox) with black to width x height;
    # "stretch": scale each axis independently.
    mode: str = "fit"

    @classmethod
    def parse(cls, size: str, mode: str = "fit") -> "ImageSpec":
        w, h = size.lower().split("x")
        return cls(int(w), int(h), mode)

    def describe(self) -> str:
        return f"{self.width}x{self.height}, {self.mode}, PIL box filter"


def prepare(frame: np.ndarray, spec: ImageSpec) -> np.ndarray:
    """Downscale a captured RGB frame to the model's input size."""
    image = Image.fromarray(frame)
    if spec.mode == "stretch":
        return np.asarray(image.resize((spec.width, spec.height), Image.Resampling.BOX))
    scale = min(spec.width / image.width, spec.height / image.height)
    w = max(1, round(image.width * scale))
    h = max(1, round(image.height * scale))
    resized = image.resize((w, h), Image.Resampling.BOX)
    canvas = Image.new("RGB", (spec.width, spec.height))
    canvas.paste(resized, ((spec.width - w) // 2, (spec.height - h) // 2))
    return np.asarray(canvas)
