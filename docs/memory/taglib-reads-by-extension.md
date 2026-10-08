---
name: taglib-reads-by-extension
description: TagLib picks its parser from the file extension; MP3 lyrics live under USLT, not LYRICS
metadata:
  type: project
---

`go.senan.xyz/taglib` (TagLib) chooses the parser from the **file extension**,
not the content. An M4A saved as `.mp3` reads as untagged: no title, no
duration, so the scanner falls back to the file name. If a song shows up
titled after its file with a duration of 0, check the extension before
suspecting the tags. Found when a test wrote M4A bytes to a `.mp3` path
(2026-10-06). Matroska/WebM is the exception: taglib has no parser for it,
so `library/tags.go` detects it by its first bytes and reads it with
`library/matroska.go` instead.

MP3 files keep embedded lyrics under the `USLT` key in the map taglib
returns, while M4A, Opus and FLAC use `LYRICS`. `library/tags.go` and
`library/media.go` check both; a new lyrics code path must too.
