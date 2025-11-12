# Binaural 8D Audio Generator

This program transforms music into an immersive 8D audio experience with spatial movement and realistic positioning effects.

## What is 8D Audio?

8D audio creates the illusion of sound moving around your head in three-dimensional space. By using HRTF (Head-Related Transfer Function) technology, this tool simulates how sound naturally reaches your ears from different positions, creating an incredibly immersive listening experience.

> [!NOTE]
> This effect only works properly with headphones. Speakers will not reproduce the spatial positioning.

## Features

- Multiple movement patterns (circular, figure-8, spiral, random, and vertical circle)
- Customizable spatial parameters (velocity, elevation, distance - can be static or oscillating)
- Reverb effects
- Bass boost
- Multiple audio formats support
- Real-time playback

## Usage

```
Usage: make8d.exe [OPTIONS] <INPUT_FILE>

Arguments:
  <INPUT_FILE>  Input audio file path

Options:
  -o, --output <OUTPUT_FILE>  Output file path (WAV, FLAC, OGG, MP3). If not specified, plays audio directly
  -h, --help                  Print help
  -V, --version               Print version

Spatial options:
  -p, --pattern <PATTERN>            Movement pattern [default: circular] [possible values: circular, figure8, spiral, random, vertical-circle]
  -a, --start-angle <DEGREES>        Starting angle in degrees, 0 - 359 [default: 0.0]
  -v, --velocity <VALUE|FROM,TO>     Movement velocity, 0 - 10 (single value or from,to range) [default: 0.2]
      --velocity-osc-speed <SPEED>   Velocity oscillation speed, 0 - 10 [default: 0.1]
  -e, --elevation <DEG|FROM,TO>      Elevation in degrees, -90 - 90 (single value or from,to range) [default: 0.0]
      --elevation-osc-speed <SPEED>  Elevation oscillation speed, 0 - 10 [default: 0.1]
  -d, --distance <METERS|FROM,TO>    Distance/radius in meters, 0.1 - 100 (single value or from,to range) [default: 1.0]
      --distance-osc-speed <SPEED>   Distance oscillation speed, 0 - 10 [default: 0.1]

Bass options:
      --crossover <FREQUENCY>  Crossover frequency in Hz, 50 - 500 [default: 200]
  -b, --bass-boost <DB>        Bass boost in dB, -20 - 20 [default: 0.0]

Reverb options:
  -r, --reverb-mix <VALUE>
          Reverb mix amount, 0.0 - 1.0 [default: 0.3]
      --reverb-room <REVERB_ROOM>
          Reverb room size, 0.0 - 1.0 [default: 0.5]
      --reverb-dampening <REVERB_DAMPENING>
          Reverb high-frequency dampening, 0.0 - 1.0 [default: 0.5]
      --reverb-width <REVERB_WIDTH>
          Reverb stereo width, 0.0 - 1.0 [default: 0.9]
```

### Basic Examples

Process an audio file with default settings:
```bash
make8d input.mp3 -o output.mp3
```

Preview without saving (plays directly):
```bash
make8d input.mp3
```

### Advanced Examples

**Slow circular movement with elevation change:**
```bash
make8d input.mp3 -o output.mp3 \
  --pattern circular \
  --velocity 0.1 \
  --elevation -20,20 \
  --elevation-osc-speed 0.2
```

**Fast figure-8 pattern with oscillating distance:**
```bash
make8d input.mp3 -o output.mp3 \
  --pattern figure-8 \
  --velocity 0.5 \
  --distance 0.5,2.0 \
  --distance-osc-speed 0.3
```

**Increased bass and reverb:**
```bash
make8d input.mp3 -o output.mp3 \
  --bass-boost 6 \
  --reverb-mix 0.5 \
  --reverb-room 0.7
```

## Tips for Best Results

1. **Start Simple**: Use default settings first, then adjust parameters
2. **Velocity**: Lower values (0.1-0.3) work well for most music
3. **Oscillation**: Use oscillation speeds between 0.05-0.3 for smooth transitions
4. **Bass Boost**: Try with 3-6 dB for electronic music, less for acoustic
5. **Reverb**: Keep reverb-mix around 0.2-0.4 to avoid muddiness
6. **Distance**: Closer distances (0.5-1.5m) create more intimate sound

## Technical Details

- **Sample Rate**: All audio is processed at 44.1 kHz
- **HRTF**: Uses IRC_1002_C impulse responses
- **Crossover**: 4th-order Linkwitz-Riley filter (-24 dB/octave) for omnidirectional bass
- **Reverb**: Freeverb algorithm

## License

MIT License
