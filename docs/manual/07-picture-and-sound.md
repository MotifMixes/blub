# Picture and sound

## Picture

**Video** chooses how the 320 x 240 picture is scaled:

- **AI upscale (HD)**: neural networks (ArtCNN, Anime4K, ScaleFX) redraw it at your window's size. **AI upscale settings** chooses the style and quality.
- **Sharp pixels**, **LCD grid**, **Pixel perfect** (whole-number scale only) and **Smooth**.

**Window size** sets the window to a multiple of the picture; `F11` is fullscreen.

## Sound

**Audio** has the volume, mute and two optional effects (Crisp, and Crisp with wide stereo). The original is the console's own sound.

## Screenshots and recordings

- `F12` saves a screenshot in the `screenshots` folder of Blub's data folder.
- `F8` records the game (with the console view when it shows) until `F8` again. With FFmpeg installed you can choose H.265, VP9 or lossless; without it, Windows makes an H.264 `.mp4` by itself.
