#!/usr/bin/env python3
"""マージ済みの PR の一覧を、cliff.toml が読む CHANGELOG_PR_MAP の TSV にする。

標準入力: `gh api graphql --paginate` の出力（pullRequests の nodes を持つ JSON の並び）
標準出力: 「マージコミットの SHA・PR の番号・題」の TSV。番号の昇順

題の後ろに、PR の本文で宣言した解決の参照（Closes #N / Closes TASK-N。Fixes / Resolves も）を
「（#N、TASK-N を解決）」として足す。本文は Markdown を自分で解釈せず、GitHub が描画した HTML
（bodyHTML）を HTML のパーサーで読む。コード・引用・取り消し線の要素の中は地の文でないので読まない
（HTML のコメントは GitHub が描画で落とす）。Markdown の書き方（フェンスの種類、字下げ、
バッククォートの数など）の違いは GitHub の描画が吸収するので、ここでは扱わない。
標準ライブラリだけを使う。
"""
import json
import re
import sys
from html.parser import HTMLParser

# この要素の中の文字は地の文ではない
SKIPPED = frozenset({"pre", "code", "blockquote", "del", "s", "strike"})
# 文字の並びが切れる要素。前後の文字をつなげない（段落の末尾の Closes と次の段落の #N を結ばない）
BREAKS = frozenset({
    "p", "div", "br", "hr", "li", "ul", "ol", "table", "tr", "td", "th", "details", "summary",
    "h1", "h2", "h3", "h4", "h5", "h6", "pre", "blockquote",
})
# GitHub が Issue を閉じる語として解釈するものと同じ。語と参照は同じ行に書く
KEYWORD = r"\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?):?[ \t]+"


class _Prose(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.depth = 0  # SKIPPED の要素の入れ子の深さ
        self.parts = []

    def handle_starttag(self, tag, attrs):
        if tag in BREAKS:
            self.parts.append("\n")
        if tag in SKIPPED:
            self.depth += 1

    def handle_endtag(self, tag):
        if tag in SKIPPED and self.depth:
            self.depth -= 1
        if tag in BREAKS:
            self.parts.append("\n")

    def handle_data(self, data):
        if not self.depth:
            self.parts.append(data)


def prose(html):
    """描画済みの HTML から、地の文の文字だけを取り出す。"""
    parser = _Prose()
    parser.feed(html or "")
    parser.close()
    return "".join(parser.parts)


def closes(html, prefix):
    """地の文で「解決」と宣言された prefix（"#" か "TASK-"）の番号を、昇順で重複なく返す。"""
    found = re.findall(KEYWORD + re.escape(prefix) + r"([0-9]+)\b", prose(html), re.IGNORECASE)
    return sorted({int(number) for number in found})


def title_with_closes(title, html):
    """題に、本文で宣言した解決の参照のうち題に無いものを足す。"""
    title = re.sub(r"[\t\n\r]", " ", title)
    refs = [f"#{n}" for n in closes(html, "#") if not re.search(rf"#{n}\b", title)]
    refs += [f"TASK-{n}" for n in closes(html, "TASK-") if not re.search(rf"TASK-{n}\b", title, re.IGNORECASE)]
    return title + (f"（{'、'.join(refs)} を解決）" if refs else "")


def read_pull_requests(stream):
    """--paginate がページごとに並べた JSON を順に読み、PR を返す。"""
    decoder = json.JSONDecoder()
    text = stream.read()
    position = 0
    while position < len(text):
        if text[position].isspace():
            position += 1
            continue
        page, position = decoder.raw_decode(text, position)
        yield from page["data"]["repository"]["pullRequests"]["nodes"]


def main():
    rows = []
    for pr in read_pull_requests(sys.stdin):
        if pr["mergeCommit"] is None:
            continue
        rows.append((pr["number"], pr["mergeCommit"]["oid"], title_with_closes(pr["title"], pr["bodyHTML"])))
    for number, sha, title in sorted(rows):
        print(sha, number, title, sep="\t")


if __name__ == "__main__":
    main()
