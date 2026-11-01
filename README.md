<div align="center">

# NyashkMeter

Master-bus loudness (LUFS, LRA, true peak) and spectrum analyser VST3 plugin with stereo and tonal balance.

[![Build](https://github.com/xslimakx/NyashkMeter/actions/workflows/build.yml/badge.svg)](https://github.com/xslimakx/NyashkMeter/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
![Format](https://img.shields.io/badge/format-VST3-8a2be2)

[**Download**](https://github.com/xslimakx/NyashkMeter/releases/latest)

</div>

<p align="center"><img src="docs/img/nyashkmeter.jpg" width="720"></p>

## Install
Use the Nyashk Launcher, or unzip the `.vst3` bundle from the latest release into your VST3 folder.

## Build
```
cargo xtask bundle -p nyashkmeter --release
```
The bundle appears in `target/bundled/`.

## License
MIT
