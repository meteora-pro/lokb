#!/usr/bin/env python3
"""
Extract Wikipedia XML dump to markdown files for lokb benchmarking.

Usage:
    # Extract all articles (Simple English Wikipedia ~230K articles)
    bzcat simplewiki.xml.bz2 | python3 wiki-to-markdown.py /tmp/wiki-md/

    # Extract first 10000 articles
    bzcat simplewiki.xml.bz2 | python3 wiki-to-markdown.py /tmp/wiki-md/ --limit 10000

    # Then load into lokb:
    lokb source add simplewiki --raw /tmp/wiki-md/ --format markdown-dir --class public
"""

import sys
import os
import re
import xml.etree.ElementTree as ET
from pathlib import Path

def clean_wikitext(text: str) -> str:
    """Basic wikitext → markdown conversion (lossy but fast)."""
    # Remove templates {{ ... }}
    depth = 0
    result = []
    i = 0
    while i < len(text):
        if text[i:i+2] == '{{':
            depth += 1
            i += 2
        elif text[i:i+2] == '}}':
            depth = max(0, depth - 1)
            i += 2
        elif depth == 0:
            result.append(text[i])
            i += 1
        else:
            i += 1
    text = ''.join(result)

    # Convert headers: == Title == → ## Title
    text = re.sub(r'^={5}\s*(.+?)\s*={5}', r'##### \1', text, flags=re.MULTILINE)
    text = re.sub(r'^={4}\s*(.+?)\s*={4}', r'#### \1', text, flags=re.MULTILINE)
    text = re.sub(r'^={3}\s*(.+?)\s*={3}', r'### \1', text, flags=re.MULTILINE)
    text = re.sub(r'^={2}\s*(.+?)\s*={2}', r'## \1', text, flags=re.MULTILINE)

    # Convert links: [[Page|text]] → text, [[Page]] → Page
    text = re.sub(r'\[\[(?:[^|\]]*\|)?([^\]]+)\]\]', r'\1', text)

    # Convert bold/italic: '''bold''' → **bold**, ''italic'' → *italic*
    text = re.sub(r"'''(.+?)'''", r'**\1**', text)
    text = re.sub(r"''(.+?)''", r'*\1*', text)

    # Convert lists: * item → - item
    text = re.sub(r'^\*\s*', '- ', text, flags=re.MULTILINE)

    # Remove HTML tags
    text = re.sub(r'<ref[^>]*>.*?</ref>', '', text, flags=re.DOTALL)
    text = re.sub(r'<ref[^/]*/>', '', text)
    text = re.sub(r'<[^>]+>', '', text)

    # Remove categories and files
    text = re.sub(r'\[\[Category:[^\]]+\]\]', '', text)
    text = re.sub(r'\[\[File:[^\]]+\]\]', '', text)
    text = re.sub(r'\[\[Image:[^\]]+\]\]', '', text)

    # Clean up whitespace
    text = re.sub(r'\n{3,}', '\n\n', text)
    return text.strip()

def safe_filename(title: str) -> str:
    """Convert article title to safe filename."""
    name = title.replace('/', '_').replace('\\', '_').replace(':', '_')
    name = re.sub(r'[<>"|?*]', '', name)
    return name[:200]  # max filename length

def main():
    if len(sys.argv) < 2:
        print("Usage: bzcat dump.xml.bz2 | python3 wiki-to-markdown.py <output_dir> [--limit N]")
        sys.exit(1)

    output_dir = Path(sys.argv[1])
    limit = None
    if '--limit' in sys.argv:
        limit = int(sys.argv[sys.argv.index('--limit') + 1])

    output_dir.mkdir(parents=True, exist_ok=True)

    # Streaming XML parse
    count = 0
    skipped = 0
    title = None
    ns = None

    for event, elem in ET.iterparse(sys.stdin, events=('end',)):
        tag = elem.tag.split('}')[-1] if '}' in elem.tag else elem.tag

        if tag == 'title':
            title = elem.text
        elif tag == 'ns':
            ns = elem.text
        elif tag == 'text' and title and ns == '0':
            text = elem.text or ''

            # Skip redirects, disambiguation, stubs
            if text.startswith('#REDIRECT') or text.startswith('#redirect'):
                skipped += 1
                elem.clear()
                continue
            if len(text) < 200:
                skipped += 1
                elem.clear()
                continue

            markdown = f"# {title}\n\n{clean_wikitext(text)}"
            filename = safe_filename(title) + '.md'
            filepath = output_dir / filename

            filepath.write_text(markdown, encoding='utf-8')
            count += 1

            if count % 5000 == 0:
                print(f"  Extracted {count} articles (skipped {skipped})...", file=sys.stderr)

            if limit and count >= limit:
                break

        elif tag == 'page':
            elem.clear()
            title = None
            ns = None

    print(f"Done: {count} articles extracted, {skipped} skipped", file=sys.stderr)
    print(f"Output: {output_dir}", file=sys.stderr)

if __name__ == '__main__':
    main()
