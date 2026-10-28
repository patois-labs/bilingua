# bilingua-cli

The `bilingua` command: try the parser library on a file without writing code.

```
bilingua extract page.md --target es-ES --mode sentence -o page.xlf
bilingua merge page.xlf -o page.es.md
bilingua segment notes.txt --source en
bilingua wordcount strings.json
bilingua pseudo page.md -o page.pseudo.md
```

| Command | What it does |
|---|---|
| `extract <file> [--source en] [--target xx] [--mode sentence\|paragraph\|document] -o out.xlf` | Writes XLIFF 2.1 and a sidecar `out.xlf.skl.json` with the skeleton (the full document model) |
| `merge <out.xlf> -o file [--skeleton path]` | Reads the `<target>`s back and writes the file in its original format |
| `segment <file> [--json]` | Prints `segment id`, a tab, and the source text (tags as `{display}`) |
| `wordcount <file>` | Words per unit and a total (locked units are listed but not totalled) |
| `pseudo <file> [-o out]` | Pseudo-translation: accented letters, 30 percent longer, in `[ ]`, tags and placeholders kept |

Formats today: `.txt`, `.json`, `.md`. The source language (default `en`) picks the sentence rules.

## XLIFF

One `<unit>` per Bilingua unit (`name` is the JSON pointer, `translate="no"` for locked units), one `<segment>`
per segment, `<ignorable>` for the text between sentences. Codes are `<pc>` when they nest inside the segment,
`<sc>`/`<ec>` when they overlap, `<ph>` for standalone ones; the original markup stays in the sidecar, the XLIFF
only carries the display text. Segment `state` is `initial`, `translated` or `final`.
On merge a target with a code the source does not have, or a repeated code, is refused with the segment id;
a missing code is allowed (it is left out, as in a CAT tool).

## Word count

A word is a run of letters and digits (apostrophes and hyphens between letters, dots and commas between digits
stay inside). Han, Hiragana and Katakana count one word per character, since those languages use no spaces; the
prolonged sound mark and middle dot are not counted. Hangul is counted by space-separated word.

## Not done

Only XLIFF 2.1 (no 1.2). No TM or glossary options. `merge` needs the sidecar file next to the XLIFF.
The sidecar is the whole document model as JSON; its shape may change until Bilingua 1.0.
