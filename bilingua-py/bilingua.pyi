"""Type stubs for the Bilingua Python module (the compiled extension `bilingua`).

A Document is plain dicts and lists, the same JSON shape as the Rust `bilingua_model::Document`. The TypedDicts
below name the keys a caller reads and writes; any key may be missing in an older document, and new keys may be
added in a 1.x release (`total=False`).
"""

from typing import Any, Literal, Mapping, TypedDict

__version__: str

Granularity = Literal["sentence", "paragraph", "document"]
OutputEncoding = Literal["keep", "utf-8", "utf8"]
SegState = Literal["new", "draft", "translated", "confirmed", "reviewed", "signed-off", "rejected", "locked"]
FormatOptionValue = str | bool | int

class TextInline(TypedDict):
    text: str

class TagBody(TypedDict, total=False):
    id: str
    kind: Literal["open", "close", "standalone"]
    display: str
    free: bool

class TagInline(TypedDict):
    tag: TagBody

Inline = TextInline | TagInline

class Origin(TypedDict, total=False):
    kind: Literal["tm", "mt", "ai", "human", "other"]
    name: str
    match_pct: int

class Segment(TypedDict, total=False):
    id: str
    source: list[Inline]
    target: list[Inline]
    state: SegState
    origin: Origin
    empty_target: bool

class UnitContext(TypedDict, total=False):
    kind: str | None
    style: str | None
    key: str | None
    heading: str | None
    location: str | None
    max_len: int | None
    notes: list[str]
    extra: dict[str, str]

class Note(TypedDict, total=False):
    author: str
    text: str
    kind: str

class Unit(TypedDict, total=False):
    id: str
    segments: list[Segment]
    separators: list[str]
    codes: dict[str, dict[str, Any]]
    context: UnitContext
    translate: bool
    notes: list[Note]
    lock_reason: str
    hidden: dict[str, str]

class Warning(TypedDict, total=False):
    kind: str
    part: str
    message: str

class Document(TypedDict, total=False):
    format: str
    name: str | None
    source_lang: str | None
    target_lang: str | None
    granularity: Granularity
    units: list[Unit]
    skeleton: dict[str, Any]
    warnings: list[Warning]

class FormatInfo(TypedDict):
    id: str
    extensions: list[str]
    bilingual: bool

class BilinguaError(Exception):
    """Every Bilingua failure. `code` is stable: "parse", "unsupported", "unknown_segment", "locked",
    "invented_tag", "duplicated_tag", "parser_version", "write", "limit", "encoding", "format_option",
    "invalid_argument", "invalid_document", "internal", ..."""

    code: str

def formats() -> list[FormatInfo]:
    """All formats, most specific first."""

def detect(name: str, data: bytes) -> str | None:
    """The format id for a file name and its bytes, or None when no parser accepts it."""

def extract(
    name: str,
    data: bytes,
    *,
    source_lang: str | None = None,
    target_lang: str | None = None,
    granularity: Granularity = "paragraph",
    limits: Mapping[str, int] | None = None,
    format_options: Mapping[str, FormatOptionValue] | None = None,
) -> Document:
    """Opens a file. `limits` changes any of the safety limits by name (`max_units`, ...). `format_options` are
    `"<format>.<name>"` keys (`"csv.columns"`, `"odf.translate_comments"`); an unknown key raises
    BilinguaError with code "format_option"."""

def merge(
    document: Mapping[str, Any],
    *,
    output_encoding: OutputEncoding = "keep",
    stamp_target_always: bool = False,
) -> bytes:
    """Writes a Document back to file bytes. `output_encoding="utf-8"` writes UTF-8 whatever the file's own
    encoding; `stamp_target_always` stamps the target language and fills plurals even when nothing is translated
    (Localization OS behaviour)."""

def version() -> str:
    """The Bilingua version."""
