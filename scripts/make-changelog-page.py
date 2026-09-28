#!/usr/bin/env python3
import html
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "CHANGELOG.md"
PAGE = ROOT / "site" / "changelog.html"

HEAD = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Changelog - Kelp</title>
<meta name="description" content="Every Kelp release, newest first: new features, speed-ups and fixes in the fast native git client for macOS.">
<link rel="canonical" href="https://kelp.hoboware.dev/changelog.html">
<meta name="theme-color" content="#15181e">
<link rel="icon" type="image/png" href="favicon.png">
<link rel="apple-touch-icon" href="apple-touch-icon.png">
<meta property="og:site_name" content="Kelp">
<meta property="og:title" content="Changelog - Kelp">
<meta property="og:description" content="What changed in each Kelp release, newest first.">
<meta property="og:type" content="article">
<meta property="og:url" content="https://kelp.hoboware.dev/changelog.html">
<meta property="og:image" content="https://kelp.hoboware.dev/kelp-og.png">
<meta name="twitter:card" content="summary_large_image">
<link rel="alternate" type="text/markdown" href="https://github.com/Hobo-Ware/kelp/blob/main/CHANGELOG.md">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<link rel="stylesheet" href="style.css">
</head>
<body>

<nav><div class="wrap">
  <a class="brand" href="index.html"><img src="mascot.svg" alt="" width="30" height="30">Kelp</a>
  <span class="version">{version}</span>
  <div class="navlinks">
    <a href="docs/getting-started.html">Docs</a>
    <a href="docs/shortcuts.html">Shortcuts</a>
    <a href="docs/faq.html">FAQ</a>
    <a href="changelog.html" aria-current="page">Changelog</a>
    <a class="gh" href="https://github.com/Hobo-Ware/kelp">GitHub</a>
  </div>
</div></nav>

<main class="doc"><div class="wrap">
  <p class="eyebrow">Changelog</p>
  <h1>What's new in Kelp</h1>
  <p class="lede">{intro} Kelp updates itself, so you usually have the newest one already.</p>
"""

FOOT = """</div></main>

<footer><div class="wrap">
  <span>Kelp is MIT licensed. Made by <a href="https://github.com/Hobo-Ware">Hoboware</a>.</span>
  <span><a href="docs/getting-started.html">Getting started</a> · <a href="docs/shortcuts.html">Shortcuts</a> · <a href="docs/undo.html">Undo</a> · <a href="docs/faq.html">FAQ</a> · <a href="changelog.html">Changelog</a> · <a href="https://github.com/Hobo-Ware/kelp">GitHub</a></span>
</div></footer>
</body>
</html>
"""


def inline(text):
    parts = re.split(r"(`[^`]+`)", text)
    return "".join(
        f"<code>{html.escape(p[1:-1], quote=False)}</code>" if p.startswith("`") else html.escape(p, quote=False)
        for p in parts
    )


def parse(markdown):
    intro, releases = [], []
    for line in markdown.splitlines():
        if line.startswith("## "):
            version, _, date = line[3:].partition(" - ")
            releases.append({"version": version.strip(), "date": date.strip(), "items": []})
        elif line.startswith("- ") and releases:
            releases[-1]["items"].append(line[2:].strip())
        elif line.startswith("  ") and releases and releases[-1]["items"]:
            releases[-1]["items"][-1] += " " + line.strip()
        elif line.strip() and not line.startswith("#") and not releases:
            intro.append(line.strip())
    return " ".join(intro), releases


def render(markdown):
    intro, releases = parse(markdown)
    out = [HEAD.replace("{version}", html.escape(releases[0]["version"])).replace("{intro}", inline(intro))]
    for release in releases:
        anchor = release["version"].replace(".", "-")
        out.append(f'  <section class="release" id="{anchor}">\n')
        out.append(f'    <h2>{html.escape(release["version"])} <time datetime="{release["date"]}">{release["date"]}</time></h2>\n')
        out.append("    <ul>\n")
        for item in release["items"]:
            out.append(f"      <li>{inline(item)}</li>\n")
        out.append("    </ul>\n  </section>\n")
    out.append(FOOT)
    return "".join(out)


def main():
    page = render(SOURCE.read_text())
    if "--check" in sys.argv[1:]:
        if not PAGE.exists() or PAGE.read_text() != page:
            print("site/changelog.html is out of date: run scripts/make-changelog-page.py", file=sys.stderr)
            return 1
        return 0
    PAGE.write_text(page)
    print(f"Wrote {PAGE.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
