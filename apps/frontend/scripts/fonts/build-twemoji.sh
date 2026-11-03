#!/usr/bin/env bash
# 絵文字の字 Twemoji（COLRv0 の WOFF2）を、jdecked/twemoji の SVG から組み直す（TASK-385）。
# build では走らせない。Twemoji の版を上げる時にだけ、手で走らせ、出来た file を commit する。
#
#   bash scripts/fonts/build-twemoji.sh
#
# 出どころ: sousuke0422/shogun-desktop の assets/fonts/build-twemoji.sh を写して変えた。
# その元は Arch Linux の AUR の twemoji-fonts の作り方
# （https://aur.archlinux.org/pkgbase/twemoji-fonts）である。nanoemoji で glyf_colr_0 に組む所は同じ。
# 変えた所:
#   1. 道具の版を scripts/fonts/twemoji-requirements.txt で固定する（同じ版から同じ字を組むため）。
#   2. VS16（U+FE0F）の有る無しを埋める名の写しを足す（下の注釈）。
#   3. 組んだ TTF を WOFF2 に縮め、@font-face を書いた CSS も書く（web で配るため）。
#   4. 刻を固定する（SOURCE_DATE_EPOCH）。同じ版から二度組んで同じ hash の字が出る。
#
# VS16 の写し:
# nanoemoji は file の名から合字を作る。Twemoji の名と browser に来る並びとで VS16（U+FE0F）の
# 有る無しが食い違うと、一つの絵にならない。browser の字の組み立てにこれを凌ぐ手は入れられない
# ので、字の側で直す。食い違いは二つの向きに在る。
#   1. Twemoji は keycap（1️⃣ など）や幾つかの絵文字を、VS16 を抜いた名（31-20e3.svg）で配る。
#      そのままでは 1 20E3 の合字しか出来ず、正しい並び 1 FE0F 20E3 では一つの絵にならない。
#   2. Twemoji は ZWJ の並びを VS16 入りの名（2764-fe0f-200d-1f525.svg）で配る。Chromium と
#      WebKit は、並びの途中の VS16 を落として組むことがあり、その時は 2764 200D 1F525 の合字が
#      要る（手元の像で、❤️‍🔥 と 🏳️‍🌈 が二つの絵に割れた）。
# Unicode の emoji-test.txt（Emoji 17.0）の並びのうち、Twemoji の名に無く、VS16 を抜くと
# VS16 を抜いた Twemoji の名と一致する物の全部に、写しを足す。fully-qualified の並びが 1 の向き、
# minimally-qualified と unqualified の並びが 2 の向きを埋める。足した並びは
# scripts/fonts/twemoji-vs16-copies.txt に書く。
#
# 出来る物:
#   public/fonts/twemoji/twemoji-<版>.woff2
#   public/fonts/twemoji/twemoji-<版>.css（@font-face。unicode-range は字の cmap から書く）
#   scripts/fonts/twemoji-vs16-copies.txt
# 名に版を入れるのは、版を上げた時に古い CSS が cache から古い字を指さないため。
# 版を上げたら、src/lib/emoji-font.ts の Twemoji の選び肢の stylesheet も新しい名に直す
# （lib/__tests__/emoji-font.test.ts が、指す file が public に在ることを確かめる）。
set -euo pipefail

TAG=v17.0.3
EMOJI_TEST_URL=https://www.unicode.org/Public/17.0.0/emoji/emoji-test.txt
EMOJI_TEST_SHA256=1d8a944f88d7952f7ef7c5167fef3c67995bcae24543949710231b03a201acda
# 字の中の刻（head の created と modified）を固定する。値は Twemoji v17.0.3 の公開の日
export SOURCE_DATE_EPOCH=1780352497

FRONTEND="$(cd "$(dirname "$0")/../.." && pwd)"
OUT_DIR="$FRONTEND/public/fonts/twemoji"
COPIES_LIST="$FRONTEND/scripts/fonts/twemoji-vs16-copies.txt"
VERSION="${TAG#v}"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/twemoji-build.XXXXXX")"
echo "work dir: $WORK"
cd "$WORK"

python3 -m venv .venv
.venv/bin/pip -q install -r "$FRONTEND/scripts/fonts/twemoji-requirements.txt"

git clone -q --depth 1 --branch "$TAG" https://github.com/jdecked/twemoji.git
curl -fsSL -o emoji-test.txt "$EMOJI_TEST_URL"
echo "$EMOJI_TEST_SHA256  emoji-test.txt" | sha256sum -c -

COPIES_LIST="$COPIES_LIST" TAG="$TAG" .venv/bin/python - <<'PY'
import os
import shutil

d = 'twemoji/assets/svg'


def nano_name(codepoints):
    """nanoemoji の名（emoji_uXXXX_YYYY.svg）。16 進は 4 桁に埋め、小文字にする。"""
    return 'emoji_u' + '_'.join(f'{cp:04x}' for cp in codepoints) + '.svg'


# Twemoji の名（1f469-200d-1f467.svg）を nanoemoji の名に改める
names = {}
for name in os.listdir(d):
    if not name.endswith('.svg'):
        continue
    codepoints = tuple(int(part, 16) for part in name[:-4].split('-'))
    new = nano_name(codepoints)
    if new != name:
        os.rename(os.path.join(d, name), os.path.join(d, new))
    names[codepoints] = new

def strip_vs16(codepoints):
    return tuple(cp for cp in codepoints if cp != 0xFE0F)


asset_count = len(names)

# VS16 を抜いた並びから、Twemoji の名への引き当て（同じ形に二つの名が当たるなら VS16 の少ない方）
by_stripped = {}
for codepoints in sorted(names, key=lambda c: (len(c), c)):
    by_stripped.setdefault(strip_vs16(codepoints), codepoints)

# emoji-test.txt の並びのうち、Twemoji の名に無く、VS16 を抜くと VS16 を抜いた Twemoji の名に
# 一致する物に、その名の写しを足す
QUALIFIED = ('fully-qualified', 'minimally-qualified', 'unqualified')
copies = []
for line in open('emoji-test.txt', encoding='utf-8'):
    head, sep, rest = line.partition(';')
    status = rest.split('#', 1)[0].strip()
    if not sep or status not in QUALIFIED:
        continue
    codepoints = tuple(int(part, 16) for part in head.split())
    if codepoints in names or len(codepoints) < 2:
        continue
    source = by_stripped.get(strip_vs16(codepoints))
    if source is None:
        continue
    shutil.copyfile(os.path.join(d, names[source]), os.path.join(d, nano_name(codepoints)))
    names[codepoints] = nano_name(codepoints)
    copies.append((codepoints, source, status))

copies.sort()
with open(os.environ['COPIES_LIST'], 'w', encoding='utf-8') as out:
    out.write(
        f"# scripts/fonts/build-twemoji.sh が足した VS16 の写し（Twemoji {os.environ['TAG']}、Emoji 17.0）。\n"
        '# 一行に一つ、写しの並び（16 進）、写した元の Twemoji の並び、emoji-test.txt の区分。手で直さない。\n'
    )
    for codepoints, source, status in copies:
        out.write(
            ' '.join(f'{cp:04X}' for cp in codepoints)
            + ' <- '
            + ' '.join(f'{cp:04X}' for cp in source)
            + f' ; {status}\n'
        )
print(f'{asset_count} svg assets, {len(copies)} VS16 copies')

files = sorted(f for f in os.listdir(d) if f.endswith('.svg'))
toml = [
    'family = "Twemoji"',
    'output_file = "TwemojiCOLRv0.ttf"',
    'color_format = "glyf_colr_0"',
    'clipbox_quantization = 32',
    '',
    '[axis.wght]', 'name = "Weight"', 'default = 400', '',
    '[master.regular]', 'style_name = "Regular"', 'srcs = [',
]
toml += [f'"{d}/{f}",' for f in files]
toml += [']', '', '[master.regular.position]', 'wght = 400', '']
open('twemoji_glyf_colrv0.toml', 'w').write('\n'.join(toml))
PY

PATH="$WORK/.venv/bin:$PATH" nanoemoji twemoji_glyf_colrv0.toml

mkdir -p "$OUT_DIR"
OUT_DIR="$OUT_DIR" VERSION="$VERSION" TAG="$TAG" .venv/bin/python - <<'PY'
import os
from fontTools.ttLib import TTFont

out_dir = os.environ['OUT_DIR']
version = os.environ['VERSION']
woff2_name = f'twemoji-{version}.woff2'
css_name = f'twemoji-{version}.css'

# 前の版の字と CSS を消す（一つの版だけを置く）
for name in os.listdir(out_dir):
    if name.startswith('twemoji') and name.endswith(('.woff2', '.css')):
        os.remove(os.path.join(out_dir, name))

font = TTFont('build/TwemojiCOLRv0.ttf')
font.flavor = 'woff2'
font.save(os.path.join(out_dir, woff2_name))

# unicode-range は字が持つ字の全部（cmap）から書く。数字や # も keycap のために持つが、
# 字の並びでは本文の字の後ろに置くので、本文の字が持つ字はそちらで描かれる。
codepoints = sorted(font.getBestCmap())
ranges = []
start = prev = codepoints[0]
for cp in codepoints[1:]:
    if cp == prev + 1:
        prev = cp
        continue
    ranges.append((start, prev))
    start = prev = cp
ranges.append((start, prev))
unicode_range = ', '.join(f'U+{a:x}' if a == b else f'U+{a:x}-{b:x}' for a, b in ranges)

css = f"""/*
 * 絵文字の字 Twemoji の @font-face。scripts/fonts/build-twemoji.sh が書く。手で直さない。
 * 絵は Twemoji {os.environ['TAG']}（Copyright Twitter, Inc. and other contributors。今の手入れは
 * https://github.com/jdecked/twemoji）で、CC-BY 4.0（https://creativecommons.org/licenses/by/4.0/）。
 * SVG を COLRv0 の字に組み直し、keycap などに VS16 入りの名の写しを足している。
 *
 * 環境設定で Twemoji を選んだ人の画面にだけ、lib/emoji-font.ts が読み込む。
 * 名は Tailwind の既定の並びの名とぶつからない独自の名にする。
 */
@font-face {{
  font-family: 'Koyori Twemoji';
  font-style: normal;
  font-weight: 400;
  font-display: swap;
  src: url('/fonts/twemoji/{woff2_name}') format('woff2');
  unicode-range: {unicode_range};
}}
"""
with open(os.path.join(out_dir, css_name), 'w', encoding='utf-8') as out:
    out.write(css)
print(f'{woff2_name}: {os.path.getsize(os.path.join(out_dir, woff2_name))} bytes, {len(codepoints)} codepoints')
PY

# 書いた CSS を repo の formatter に通す（lint-staged と同じ形にそろえる。pnpm は Linux 側の物）
(cd "$FRONTEND" && pnpm exec vp fmt "$OUT_DIR/twemoji-$VERSION.css" >/dev/null)

echo
sha256sum build/TwemojiCOLRv0.ttf "$OUT_DIR/twemoji-$VERSION.woff2" "$OUT_DIR/twemoji-$VERSION.css"
echo "work dir: $WORK"
