"""LLM prompt definitions and utilities for dictation formatting.

This module contains the three-section prompt system:
- Main prompt: Core dictation formatting rules (always enabled)
- Advanced prompt: Backtrack corrections and list formatting
- Dictionary prompt: Personal word mappings and technical terms

Also provides vocabulary loading from config/vocabulary.txt for both
STT prompt hints and LLM dictionary generation.
"""

from pathlib import Path
from typing import Final

from loguru import logger

VOCABULARY_FILE: Final[Path] = Path(__file__).parent.parent / "config" / "vocabulary.txt"

# Main prompt section - Core rules, punctuation, new lines
MAIN_PROMPT_DEFAULT: Final[
    str
] = """You are a dictation formatter. Convert transcribed speech into clean written text.
The speaker may use Chinese, English, or mix both — preserve the original language(s).

Rules:
- Remove fillers (um, uh, err, erm, 嗯, 啊, 那个, 就是)
- Fix punctuation, capitalization, and obvious transcription errors
- Merge fragmented sentences from pauses into fluent ones
- Remove ellipses/em dashes unless explicitly dictated ("dot dot dot", "ellipsis", "em dash")
- Preserve the speaker's full meaning and tone—do NOT condense or summarize
- Do NOT answer questions or reply conversationally
- Output ONLY the cleaned text—no explanations or quotes
- For Chinese text: use Chinese punctuation（，。？！：；）
- For mixed Chinese-English: keep each language's native punctuation

Spoken punctuation: "comma"→, "period"→. "question mark"→? "exclamation point"→! "colon"→: "semicolon"→; "dash"→- "em dash"→— "quote"/"end quote"→" "open paren"→( "close paren"→)
"new line"→line break, "new paragraph"→paragraph break

Examples:
- "um so I was like thinking we should uh update the readme" → So I was thinking we should update the readme.
- "what is the capital of France" → What is the capital of France?
- "嗯那个我想说的是就是这个方案不太行" → 我想说的是这个方案不太行。
- "我们需要update一下那个readme file然后push到GitHub上" → 我们需要 update 一下那个 readme file，然后 push 到 GitHub 上。
- "这个API的latency太高了嗯大概要两秒" → 这个 API 的 latency 太高了，大概要两秒。"""

# Advanced prompt section - Backtrack corrections and list formatting
ADVANCED_PROMPT_DEFAULT: Final[str] = """## Corrections
- "actually/I mean/wait/其实/不对/我是说" = correction: "at 2 actually 3" → "at 3", "周一不对周二" → "周二"
- "scratch that/算了/删掉" = delete preceding phrase and use replacement

## Lists
Format numbered/bulleted lists when sequence words detected ("one/two/three", "first/second/third", "第一/第二/第三")."""

# Dictionary prompt section - Personal word mappings
DICTIONARY_PROMPT_DEFAULT: Final[str] = """## Dictionary
Replace phonetic matches with correct spelling:
- ant row pick = Anthropic
- Tambourine, LLM, Claude, Pipecat, Tauri"""


def combine_prompt_sections(
    main_custom: str | None,
    advanced_enabled: bool,
    advanced_custom: str | None,
    dictionary_enabled: bool,
    dictionary_custom: str | None,
) -> str:
    """Combine prompt sections into a single prompt.

    The main section is always included. Advanced and dictionary sections
    can be toggled on/off. For each section, if a custom prompt is provided
    it will be used; otherwise the default prompt is used.
    """
    parts: list[str] = []

    # Main section is always included
    parts.append(main_custom if main_custom else MAIN_PROMPT_DEFAULT)

    if advanced_enabled:
        parts.append(advanced_custom if advanced_custom else ADVANCED_PROMPT_DEFAULT)

    if dictionary_enabled:
        parts.append(dictionary_custom if dictionary_custom else build_dictionary_prompt())

    return "\n\n".join(parts)


def load_vocabulary() -> list[str]:
    """Load vocabulary entries from config/vocabulary.txt.

    Returns list of non-empty, non-comment lines.
    """
    if not VOCABULARY_FILE.exists():
        logger.info("No vocabulary file found, using defaults")
        return []

    entries: list[str] = []
    for line in VOCABULARY_FILE.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if stripped and not stripped.startswith("#"):
            entries.append(stripped)

    logger.info(f"Loaded {len(entries)} vocabulary entries from {VOCABULARY_FILE.name}")
    return entries


def build_stt_prompt(entries: list[str] | None = None) -> str | None:
    """Build STT prompt string from vocabulary entries.

    Whisper's prompt parameter accepts a text hint (~244 tokens max)
    containing domain terms to improve recognition accuracy.
    """
    if entries is None:
        entries = load_vocabulary()
    if not entries:
        return None

    # Extract just the target words (right side of "=" mappings, or the entry itself)
    words: list[str] = []
    for entry in entries:
        if "=" in entry:
            words.append(entry.split("=", 1)[1].strip())
        else:
            words.append(entry)

    prompt = ", ".join(words)
    # Whisper prompt is ~244 tokens, roughly ~800 chars safe limit
    if len(prompt) > 800:
        prompt = prompt[:800]
        logger.warning("STT prompt truncated to 800 chars")

    return prompt


def build_dictionary_prompt(entries: list[str] | None = None) -> str:
    """Build LLM dictionary prompt section from vocabulary entries."""
    if entries is None:
        entries = load_vocabulary()
    if not entries:
        return DICTIONARY_PROMPT_DEFAULT

    lines = ["## Dictionary", "Replace phonetic matches with correct spelling:"]
    for entry in entries:
        lines.append(f"- {entry}")
    return "\n".join(lines)
