#!/usr/bin/env python3
import html.parser
import pathlib
import sys
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
ORIGIN = "https://kelp.hoboware.dev/"
DASHES = (chr(0x2013), chr(0x2014))


class Page(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.links, self.ids, self.meta, self.title, self._in_title = [], set(), {}, "", False

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        for key in ("href", "src"):
            if attrs.get(key) and not (tag == "link" and attrs.get("rel") in ("preconnect", "alternate")):
                self.links.append(attrs[key])
        if tag == "meta":
            name = attrs.get("name") or attrs.get("property")
            if name:
                self.meta[name] = attrs.get("content", "")
        if tag == "link" and attrs.get("rel") == "canonical":
            self.meta["canonical"] = attrs.get("href", "")
        self._in_title = tag == "title"

    def handle_endtag(self, tag):
        self._in_title = False

    def handle_data(self, data):
        if self._in_title:
            self.title += data


def parse(path):
    page = Page()
    page.feed(path.read_text())
    return page


def main():
    external = "--external" in sys.argv[1:]
    problems = []
    pages = {path: parse(path) for path in sorted(SITE.rglob("*.html"))}
    sitemap = (SITE / "sitemap.xml").read_text()
    outside = set()
    for path, page in pages.items():
        name = path.relative_to(SITE).as_posix()
        url = ORIGIN if name == "index.html" else ORIGIN + name
        if f"<loc>{url}</loc>" not in sitemap:
            problems.append(f"{name}: not in sitemap.xml")
        if not page.title.strip():
            problems.append(f"{name}: no <title>")
        for key in ("description", "canonical", "og:title", "og:description", "og:url", "og:image"):
            if not page.meta.get(key):
                problems.append(f"{name}: no {key}")
        if page.meta.get("canonical") != url or page.meta.get("og:url") != url:
            problems.append(f"{name}: canonical and og:url should be {url}")
        for link in page.links:
            if link.startswith(ORIGIN):
                own = link[len(ORIGIN):].split("#")[0] or "index.html"
                if not (SITE / own).exists():
                    problems.append(f"{name}: {link} has no file in site/")
                continue
            parsed = urllib.parse.urlparse(link)
            if parsed.scheme in ("http", "https"):
                outside.add(link)
                continue
            if parsed.scheme in ("mailto", "data"):
                continue
            target = (path.parent / urllib.parse.unquote(parsed.path)).resolve() if parsed.path else path
            if not target.exists():
                problems.append(f"{name}: broken link {link}")
            elif parsed.fragment and target.suffix == ".html":
                if parsed.fragment not in pages[target].ids:
                    problems.append(f"{name}: no #{parsed.fragment} in {target.relative_to(SITE)}")
    for path in [ROOT / "CHANGELOG.md", *SITE.rglob("*")]:
        if path.is_file() and path.suffix in (".html", ".css", ".md", ".xml", ".txt", ".svg"):
            for n, line in enumerate(path.read_text().splitlines(), 1):
                if any(d in line for d in DASHES):
                    problems.append(f"{path.relative_to(ROOT)}:{n}: em or en dash")
    if external:
        for link in sorted(outside):
            if link.startswith(("https://fonts.googleapis.com", "https://fonts.gstatic.com")):
                continue
            request = urllib.request.Request(link, method="GET", headers={"User-Agent": "kelp-site-check"})
            try:
                with urllib.request.urlopen(request, timeout=15) as response:
                    if response.status >= 400:
                        problems.append(f"{link}: HTTP {response.status}")
            except Exception as error:
                problems.append(f"{link}: {error}")
    for problem in problems:
        print(problem, file=sys.stderr)
    checked = sum(len(p.links) for p in pages.values())
    print(f"{len(pages)} pages, {checked} links, {len(problems)} problems")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
