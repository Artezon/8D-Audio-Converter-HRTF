# Binaural 8D Audio Generator

This program transforms music into an immersive 8D audio experience with spatial movement and realistic positioning effects.

## What is 8D Audio?

8D audio creates the illusion of sound moving around your head in three-dimensional space. By using HRTF (Head-Related Transfer Function) technology, this tool simulates how sound naturally reaches your ears from different positions, creating an incredibly immersive listening experience.

> [!NOTE]
> This effect only works properly with headphones. Speakers will not reproduce the spatial positioning.

## Features

- Multiple movement patterns (circular, figure-8, vertical circle, helix, and random)
- Customizable spatial parameters (velocity, elevation, distance - can be static or oscillating)
- Reverb effects
- Bass boost
- Multiple audio formats support
- Real-time playback

## Usage

```
Usage: make8d [OPTIONS] <INPUT_FILE>

Arguments:
  <INPUT_FILE>  Input audio file path

Options:
  -o, --output <OUTPUT_FILE>  Output file path (WAV, FLAC, OGG, MP3). If not specified, plays audio directly
  -h, --help                  Print help
  -V, --version               Print version

Spatial options:
  -p, --pattern <PATTERN>            Movement pattern [default: circular] [possible values: circular, figure8, vertical-circle, helix, random]
  -a, --start-angle <DEGREES>        Starting angle in degrees, 0 - 359 [default: 0]
  -v, --velocity <RPM|FROM,TO>       Rotation velocity, -100 - 100 RPM, positive - clockwise, negative - counterclockwise (single value or from,to range) [default: 15]
      --velocity-osc-speed <SPEED>   Velocity oscillation speed, 0 - 10 [default: 5]
  -e, --elevation <DEG|FROM,TO>      Elevation in degrees, -90 - 90 (single value or from,to range) [default: 0]
      --elevation-osc-speed <SPEED>  Elevation oscillation speed, 0 - 10 [default: 5]
  -d, --distance <METERS|FROM,TO>    Distance/radius in meters, 0.1 - 100 (single value or from,to range) [default: 1]
      --distance-osc-speed <SPEED>   Distance oscillation speed, 0 - 10 [default: 0.1]

Bass options:
  -b, --bass-boost <DB>        Bass boost in dB, -20 - 20 [default: 0]
      --crossover <FREQUENCY>  Crossover frequency in Hz, 50 - 500 [default: 200]

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
make8d input.mp3 -o output.mp3 --velocity 5 --elevation -20,20 --elevation-osc-speed 2
```

**Fast figure-8 pattern with oscillating distance:**
```bash
make8d input.mp3 -o output.mp3 --pattern figure8 --velocity 30 --distance 0.5,2.0 --distance-osc-speed 0.3
```

**Increased bass and reverb:**
```bash
make8d input.mp3 -o output.mp3 --bass-boost 3 --reverb-mix 0.7 --reverb-room 0.9 --reverb-dampening 0 --reverb-width 1
```

## Technical Details

- **Sample Rate**: All audio is processed at 44.1 kHz
- **HRTF**: Uses IRC_1002_C impulse responses
- **Crossover**: 4th-order Linkwitz-Riley filter (-24 dB/octave) for omnidirectional bass
- **Reverb**: Freeverb algorithm

## License

MIT License
