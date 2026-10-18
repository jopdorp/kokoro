import hashlib
import re
from pathlib import Path
from urllib.request import urlretrieve


ASSETS = "https://raw.githubusercontent.com/jopdorp/oboromi/assets/"
TEASERS = ("teaser-dungeon.gif", "teaser-raytraced.gif", "teaser-sprites.gif")


def on_post_build(config):
    media = Path(config.site_dir) / "media"
    media.mkdir(exist_ok=True)
    pages = (Path(config.docs_dir) / "index.md", Path(config.docs_dir) / "architecture/demos.md")
    urls = {url for page in pages for url in re.findall(rf'{ASSETS}[^" )]+\.(?:mp4|png)', page.read_text())}
    urls.update(ASSETS + name for name in TEASERS)
    for url in sorted(urls):
        urlretrieve(url, media / url.rsplit("/", 1)[-1])

    page = Path(config.site_dir) / "architecture/demos/index.html"
    html = page.read_text()
    public = config.site_url.rstrip("/") + "/media/"
    for url in urls:
        name = url.rsplit("/", 1)[-1]
        versioned = f"{public}{name}?v={hashlib.sha256((media / name).read_bytes()).hexdigest()[:8]}"
        html = html.replace(public + name, versioned).replace(url, versioned)
    page.write_text(html)
