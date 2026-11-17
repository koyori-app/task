"""changelog_pr_map の単体テスト。python3 -m unittest scripts/changelog_pr_map_test.py

changelog-fixtures/closes.md は、解決の参照を「拾う書き方」と「拾わない書き方」で並べた本文。
closes.html は、それを GitHub に描画させた結果（POST /markdown、mode=gfm）を保存したもの。
環境変数 CHANGELOG_RENDERED_FIXTURE に、closes.md を今の GitHub に描画させたファイルを渡すと、
それでも同じ結果になることを確かめる（scripts/changelog.sh check が渡す）。
"""
import os
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import changelog_pr_map as pr_map  # noqa: E402

FIXTURES = pathlib.Path(__file__).parent / "changelog-fixtures"
# closes.md で地の文に書いた宣言だけ
EXPECTED_ISSUES = [1, 11, 19]
EXPECTED_TASKS = [1, 8, 10, 12, 18, 24, 25]


class ClosesTest(unittest.TestCase):
    def test_takes_only_the_declarations_in_prose_from_the_rendered_fixture(self):
        html = (FIXTURES / "closes.html").read_text(encoding="utf-8")
        self.assertEqual(pr_map.closes(html, "#"), EXPECTED_ISSUES)
        self.assertEqual(pr_map.closes(html, "TASK-"), EXPECTED_TASKS)

    @unittest.skipUnless(os.environ.get("CHANGELOG_RENDERED_FIXTURE"), "今の GitHub の描画結果が渡されていない")
    def test_takes_the_same_declarations_from_the_current_rendering(self):
        html = pathlib.Path(os.environ["CHANGELOG_RENDERED_FIXTURE"]).read_text(encoding="utf-8")
        self.assertEqual(pr_map.closes(html, "#"), EXPECTED_ISSUES)
        self.assertEqual(pr_map.closes(html, "TASK-"), EXPECTED_TASKS)

    def test_skips_code_quotes_and_struck_text(self):
        for html in (
            "<pre><code>Closes #1</code></pre>",
            "<p><code>Closes #1</code></p>",
            "<blockquote><p>Closes #1</p><blockquote><p>Closes #1</p></blockquote></blockquote>",
            "<p><del>Closes #1</del></p>",
            "<div class=\"highlight\"><pre>Closes <span>#1</span></pre></div>",
            "<p><s>Closes #1</s> <strike>Closes #1</strike></p>",
            "<p><tt>Closes #1</tt> <samp>Closes #1</samp> <kbd>Closes #1</kbd> <var>Closes #1</var></p>",
            "<math-renderer class=\"js-display-math\">$$Closes <a>#1</a>$$</math-renderer>",
        ):
            with self.subTest(html=html):
                self.assertEqual(pr_map.closes(html, "#"), [])

    def test_resumes_after_a_skipped_element(self):
        html = "<blockquote><p>Closes #1</p></blockquote><p>Closes <a href=\"x\">#2</a></p><pre>Closes #3</pre><p>fixes TASK-4</p>"
        self.assertEqual(pr_map.closes(html, "#"), [2])
        self.assertEqual(pr_map.closes(html, "TASK-"), [4])

    def test_does_not_join_the_text_around_a_skipped_element(self):
        # 中身を捨てた要素の前後をつなげると、本文に無い宣言ができる
        self.assertEqual(pr_map.closes("<p>Closes <code>#123</code> #456</p>", "#"), [])
        self.assertEqual(pr_map.closes("<p>Closes <code>TASK-123</code> TASK-456</p>", "TASK-"), [])
        self.assertEqual(pr_map.closes("<p>Closes TASK-12<code>x</code>3</p>", "TASK-"), [])
        self.assertEqual(pr_map.closes("<p>Closes <del>#1</del> #2</p>", "#"), [])
        # 要素の後ろに独立して書いた宣言は拾う
        html = "<p>Closes <code>TASK-123</code> TASK-456. Fixes TASK-789</p><p><code>x</code> fixes #790</p>"
        self.assertEqual(pr_map.closes(html, "TASK-"), [789])
        self.assertEqual(pr_map.closes(html, "#"), [790])

    def test_takes_declarations_next_to_japanese_text(self):
        # 語と参照の境界は ASCII の英数字で見る（日本語の文字は語の一部と見ない）
        self.assertEqual(pr_map.closes("<p>Closes TASK-123の対応</p>", "TASK-"), [123])
        self.assertEqual(pr_map.closes("<p>これでCloses TASK-5</p>", "TASK-"), [5])
        self.assertEqual(pr_map.closes("<p>Closes <a>#12</a>を直す</p>", "#"), [12])
        # ASCII の英数字が続くものは別の語・別の番号
        self.assertEqual(pr_map.closes("<p>Closes TASK-12a, xCloses TASK-13</p>", "TASK-"), [])

    def test_does_not_join_a_keyword_and_a_reference_across_blocks_or_lines(self):
        self.assertEqual(pr_map.closes("<p>Closes</p><p>#1</p>", "#"), [])
        self.assertEqual(pr_map.closes("<p>Closes<br>\n#1</p>", "#"), [])
        self.assertEqual(pr_map.closes("<ul><li>Closes</li><li>TASK-1</li></ul>", "TASK-"), [])
        self.assertEqual(pr_map.closes("<dl><dt>Closes </dt><dd>#1</dd></dl>", "#"), [])

    def test_needs_a_keyword_for_each_reference(self):
        self.assertEqual(pr_map.closes("<p>Closes #1, #2 and TASK-3. Resolves: TASK-4, see TASK-5</p>", "#"), [1])
        self.assertEqual(pr_map.closes("<p>Closes #1, #2 and TASK-3. Resolves: TASK-4, see TASK-5</p>", "TASK-"), [4])
        self.assertEqual(pr_map.closes("<p>discloses #1, prefixes TASK-2</p>", "#"), [])

    def test_ignores_case_and_sorts_without_duplicates(self):
        self.assertEqual(pr_map.closes("<p>FIXED task-9<br>closes TASK-10</p><p>Closes TASK-9</p>", "TASK-"), [9, 10])

    def test_handles_an_empty_body(self):
        self.assertEqual(pr_map.closes("", "#"), [])
        self.assertEqual(pr_map.closes(None, "#"), [])


class TitleTest(unittest.TestCase):
    def test_appends_only_the_references_missing_from_the_title(self):
        html = "<p>Closes #151</p><p>Closes TASK-509</p><p>Closes TASK-489</p><p>Closes #7</p>"
        self.assertEqual(
            pr_map.title_with_closes("feat: x (#151 Phase 4) (task-509)", html),
            "feat: x (#151 Phase 4) (task-509)（#7、TASK-489 を解決）",
        )

    def test_finds_a_reference_in_the_title_next_to_japanese_text(self):
        self.assertEqual(pr_map.title_with_closes("fix: y (TASK-509の対応)", "<p>Closes TASK-509</p>"), "fix: y (TASK-509の対応)")
        self.assertEqual(pr_map.title_with_closes("fix: #151の続き", "<p>Closes #151</p>"), "fix: #151の続き")

    def test_keeps_the_title_when_nothing_is_declared(self):
        self.assertEqual(pr_map.title_with_closes("fix: a\tb", "<p>TASK-1 の続き。#2 を参照</p>"), "fix: a b")

    def test_does_not_take_a_longer_number_in_the_title_for_the_same_reference(self):
        self.assertEqual(pr_map.title_with_closes("fix: y (TASK-1400)", "<p>Closes TASK-140</p>"), "fix: y (TASK-1400)（TASK-140 を解決）")


if __name__ == "__main__":
    unittest.main()
