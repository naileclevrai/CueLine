# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Sample-accurate SMPTE LTC generator at 23.976, 24, 25, 29.97 DF/ND and 30 fps, with user bits.
- MIDI Timecode output scheduled against a DLL-filtered audio clock, with offset compensation.
- Multitrack timeline: waveforms, volume, pan, mute, solo, frame-snapped clip dragging.
- Markers / cue list, next-cue countdown and a big timecode window.
- Output routing for the program mix and LTC; WASAPI by default, optional ASIO.
- Offline 24-bit WAV export (LTC only, mix + LTC, stereo mix + LTC).
- `.cueline` JSON projects with relative media paths, undo/redo, drag & drop.
- macOS-style interface: frameless window, unified toolbar with LCD, sheets, cue source list,
  San Francisco when installed and embedded Inter (OFL) otherwise.
