#!/usr/bin/env python3
from __future__ import annotations

import os
import re
from pathlib import Path
from urllib.parse import unquote


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
DOCUMENT_PAIRS = (
    ("README.md", "README.zh-CN.md"),
    ("CHANGELOG.md", "CHANGELOG.zh-CN.md"),
    ("CONTRIBUTING.md", "CONTRIBUTING.zh-CN.md"),
    ("SECURITY.md", "SECURITY.zh-CN.md"),
    ("CODE_OF_CONDUCT.md", "CODE_OF_CONDUCT.zh-CN.md"),
    ("docs/ARCHITECTURE.md", "docs/zh-CN/ARCHITECTURE.md"),
    ("docs/CI_USAGE.md", "docs/zh-CN/CI_USAGE.md"),
    ("docs/JSON_SCHEMA.md", "docs/zh-CN/JSON_SCHEMA.md"),
    ("docs/LIMITATIONS.md", "docs/zh-CN/LIMITATIONS.md"),
    ("docs/MARKET_POSITION.md", "docs/zh-CN/MARKET_POSITION.md"),
    ("docs/PRODUCT_SPEC.md", "docs/zh-CN/PRODUCT_SPEC.md"),
    ("docs/ROADMAP.md", "docs/zh-CN/ROADMAP.md"),
    ("docs/SCENARIOS.md", "docs/zh-CN/SCENARIOS.md"),
    ("docs/SECURITY_MODEL.md", "docs/zh-CN/SECURITY_MODEL.md"),
    ("docs/demo/README.md", "docs/zh-CN/demo/README.md"),
)
MARKDOWN_LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")
CJK_CHARACTER = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff]")


def relative_link(source: Path, destination: Path) -> str:
    return Path(os.path.relpath(destination, start=source.parent))


def verify_pairs(errors: list[str]) -> None:
    for english_name, chinese_name in DOCUMENT_PAIRS:
        english = REPOSITORY_ROOT / english_name
        chinese = REPOSITORY_ROOT / chinese_name
        if not english.is_file():
            errors.append(f"missing English document: {english_name}")
            continue
        if not chinese.is_file():
            errors.append(f"missing Chinese document: {chinese_name}")
            continue

        english_text = english.read_text(encoding="utf-8")
        chinese_text = chinese.read_text(encoding="utf-8")
        if not CJK_CHARACTER.search(chinese_text):
            errors.append(f"Chinese document contains no CJK text: {chinese_name}")

        english_to_chinese = relative_link(english, chinese).as_posix()
        chinese_to_english = relative_link(chinese, english).as_posix()
        if english_to_chinese not in english_text:
            errors.append(f"missing Chinese navigation link in {english_name}: {english_to_chinese}")
        if chinese_to_english not in chinese_text:
            errors.append(f"missing English navigation link in {chinese_name}: {chinese_to_english}")


def verify_local_links(errors: list[str]) -> None:
    for document in sorted(REPOSITORY_ROOT.rglob("*.md")):
        if ".git" in document.parts or "target" in document.parts:
            continue
        text = document.read_text(encoding="utf-8")
        for raw_target in MARKDOWN_LINK.findall(text):
            target = raw_target.strip().split(maxsplit=1)[0].strip("<>")
            if (
                not target
                or target.startswith("#")
                or target.startswith(("http://", "https://", "mailto:"))
            ):
                continue
            path_text = unquote(target.split("#", 1)[0])
            resolved = (document.parent / path_text).resolve()
            try:
                resolved.relative_to(REPOSITORY_ROOT)
            except ValueError:
                errors.append(f"{document.relative_to(REPOSITORY_ROOT)} links outside repository: {target}")
                continue
            if not resolved.exists():
                errors.append(f"{document.relative_to(REPOSITORY_ROOT)} has broken link: {target}")


def main() -> None:
    errors: list[str] = []
    verify_pairs(errors)
    verify_local_links(errors)
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Verified {len(DOCUMENT_PAIRS)} bilingual document pairs and all local Markdown links.")


if __name__ == "__main__":
    main()
