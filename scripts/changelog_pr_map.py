#!/usr/bin/env python3
"""マージ済みの PR の一覧を、cliff.toml が読む CHANGELOG_PR_MAP の TSV にする。

標準入力: `gh api graphql --paginate` の出力（pullRequests の nodes を持つ JSON の並び）
標準出力: 「マージコミットの SHA・PR の番号・題」の TSV。番号の昇順

題の後ろに、PR の本文で宣言した解決の参照（Closes #N / Closes TASK-N。Fixes / Resolves も）を
「（#N、TASK-N を解決）」として足す。本文は Markdown を自分で解釈せず、GitHub が描画した HTML
（bodyHTML）を HTML のパーサーで読む。コード・引用・取り消し線・数式の要素の中は地の文でないので
読まず、その前後の地の文もつなげない（HTML のコメントは GitHub が描画で落とす）。
マージコミットの無い PR は載せず、件数と番号を標準エラーへ出す。Markdown の書き方（フェンスの種類、字下げ、
バッククォートの数など）の違いは GitHub の描画が吸収するので、ここでは扱わない。
標準ライブラリだけを使う。
"""
import json
import re
import sys
from html.parser import HTMLParser

# この要素の中の文字は地の文ではない（コード・引用・取り消し線・数式）。前後の地の文もつなげない
SKIPPED = frozenset({
    "pre", "code", "tt", "samp", "kbd", "var", "math-renderer", "blockquote", "del", "s", "strike",
})
# 文字の並びが切れる要素。前後の文字をつなげない（段落の末尾の Closes と次の段落の #N を結ばない）
BREAKS = frozenset({
    "p", "div", "br", "hr", "li", "ul", "ol", "table", "tr", "td", "th", "details", "summary",
    "h1", "h2", "h3", "h4", "h5", "h6", "dl", "dt", "dd", "pre", "blockquote",
    # 脚注の番号（Closes TASK-3[^1] の 1）と上付き・下付きは、参照の数字と別のもの
    "sup", "sub",
})
# 読まない要素の前後に置く文字。空白でも英数字でもないので、語と参照の間の [ \t]+ も、
# 参照の前後の境界も越えられない。中身を捨てただけだと「Closes <code>#1</code> #2」が
# 「Closes  #2」に、「Closes TASK-12<code>x</code>3」が「Closes TASK-123」になり、無い宣言ができる
GAP = "\ufffc"
# 中を読むそのほかの要素（a・strong・em・span など）の前後に置く文字。語と参照の間には
# あってよい（「**Closes** <a>#12</a>」を拾う）が、参照の数字の後ろの境界はこれを越えて見る。
# 置かないと「Fixes TASK-5**6**」が「TASK-56」になる
MARK = "\u2060"
# 語と参照の境界は ASCII の英数字（と GAP）だけで見る（re の \b は日本語の文字も語の一部と見るので、
# 「これでCloses TASK-5」「Closes TASK-123の対応」を拾えない）
BEFORE = r"(?<![A-Za-z0-9_\ufffc])(?<![A-Za-z0-9_\ufffc]\u2060)"
AFTER = r"(?!\u2060*[A-Za-z0-9_\ufffc])"
# GitHub が Issue を閉じる語として解釈するものと同じ。語と参照は同じ行に書く
KEYWORD = BEFORE + r"(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\u2060*:?\u2060*[ \t][ \t\u2060]*"


class _Prose(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.depth = 0  # SKIPPED の要素の入れ子の深さ
        self.parts = []

    def handle_starttag(self, tag, attrs):
        if tag in SKIPPED:
            self.parts.append(GAP)
            self.depth += 1
        elif tag not in BREAKS:
            self.parts.append(MARK)
        if tag in BREAKS:
            self.parts.append("\n")

    def handle_endtag(self, tag):
        if tag in BREAKS:
            self.parts.append("\n")
        elif tag not in SKIPPED:
            self.parts.append(MARK)
        if tag in SKIPPED:
            self.parts.append(GAP)
            if self.depth:
                self.depth -= 1

    def handle_data(self, data):
        if not self.depth:
            self.parts.append(data)


def prose(html):
    """描画済みの HTML から、地の文の文字だけを取り出す。"""
    parser = _Prose()
    parser.feed(html or "")
    parser.close()
    # 入れ子の要素で続いた MARK を 1 つにする（BEFORE は MARK 1 つ分しか戻って見ない）
    return re.sub(MARK + "+", MARK, "".join(parser.parts))


def closes(html, prefix):
    """地の文で「解決」と宣言された prefix（"#" か "TASK-"）の番号を、昇順で重複なく返す。"""
    found = re.findall(KEYWORD + re.escape(prefix) + r"([0-9]+)" + AFTER, prose(html), re.IGNORECASE)
    return sorted({int(number) for number in found})


def title_with_closes(title, html):
    """題に、本文で宣言した解決の参照のうち題に無いものを足す。"""
    title = re.sub(r"[\t\n\r]", " ", title)
    refs = [f"#{n}" for n in closes(html, "#") if not re.search(rf"#{n}{AFTER}", title)]
    refs += [
        f"TASK-{n}" for n in closes(html, "TASK-")
        if not re.search(rf"{BEFORE}TASK-{n}{AFTER}", title, re.IGNORECASE)
    ]
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
    unplaced = []
    for pr in read_pull_requests(sys.stdin):
        # マージコミットの無い PR は履歴のどこにも結べない。黙って落とさず、番号を知らせる
        if pr["mergeCommit"] is None:
            unplaced.append(pr["number"])
            continue
        rows.append((pr["number"], pr["mergeCommit"]["oid"], title_with_closes(pr["title"], pr["bodyHTML"])))
    for number, sha, title in sorted(rows):
        print(sha, number, title, sep="\t")
    if unplaced:
        numbers = "、".join(f"#{n}" for n in sorted(unplaced))
        print(f"マージコミットが無いので載せない PR が {len(unplaced)} 件ある: {numbers}", file=sys.stderr)


if __name__ == "__main__":
    main()
