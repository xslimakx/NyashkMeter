## NyashkMeter 1.0.5

- Спектр у режимі CURVE тепер плавний на низах: крива між бінами FFT замість прямих відрізків.

## NyashkMeter 1.0.4

- Кілька копій плагіна в одному проєкті тепер відкриваються миттєво: раніше друге вікно відкривалося до 1,5 с, а закриття будь-якого вікна блокувало DAW до 1 с (вікно чекало, поки закриються всі вікна наших плагінів у процесі, а не лише своє). Через це в Bitwig перше вікно могло на секунду зникати.

## NyashkMeter 1.0.3

- Захист від NaN / inf від хоста: один «битий» семпл більше не ламає плагін назавжди (раніше фільтри залишались у NaN і звук пропадав до перезапуску). Тепер один NaN не «отруює» інтегровану гучність, LRA і true peak.

## NyashkMeter 1.0.2

- RMS readouts average over ~300 ms at any sample rate (they were twice as fast at 96 kHz).
- New reference tests: EBU Tech 3341 cases 1-5 read within 0.1 LU at 44.1 / 48 / 96 kHz, EBU Tech 3342 loudness range cases give exactly 10 / 5 / 20 / 15 LU, true peak of sines whose samples miss the crest reads within -0.1 / +0.15 dB.

## NyashkMeter 1.0.1

- **Loudness history:** quiet passages and silence no longer draw a line along the bottom edge, below the lowest scale label. The scale now goes down to -60 LUFS with a -50 label, anything quieter leaves a gap, and the target label has its own backing so the lines no longer run through its text.
- **Alt-tab fix (Linux):** after alt-tab, minimising or covering the window, parts of the picture (backgrounds, knobs, images) could stay blank until something on them moved. The window is now drawn again in full whenever the system throws its contents away, and pictures are no longer lost while the window is hidden or a frame is skipped.
- Smoother images in the CPU renderer (used when Vulkan is not available): pictures drawn smaller than their real size, such as knobs and photos, are now scaled with mipmaps instead of picking single pixels, so they no longer look grainy or jagged.
- **Display fix:** the built-in CPU renderer (used when Vulkan is not available) drew pixels on the shared edges of triangles twice, which showed as brighter seams and specks in see-through fills (spectra, curves, bars). It now uses an exact fill rule like a GPU: every pixel is drawn once, without holes. (Seen as dots in the spectrum fill and dotted lines inside the 1/3-octave bars.)
- Value boxes have a small gap between them, and a value that would not fit its box is drawn smaller instead of running over the frame.
- Clicking a value box and pressing Enter no longer changes a value whose unit matters (for example "1.03 s" was read back as 1.03 ms).
- New automatic test: every parameter's text is typed back over its whole range and must give the same value.

## NyashkMeter 1.0.0

First release. VST3 for Linux, Windows and macOS.
