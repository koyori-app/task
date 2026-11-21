import { Prec } from '@codemirror/state';
import { EditorView } from '@codemirror/view';

/**
 * Ctrl / ⌘+Enter の押しっぱなし（`repeat`）を飲み込む。送信の束（keymap）の `run` は event を受けず
 * repeat を見られないので、keymap より先に keydown を見て止める。送信が失敗して入力欄が戻ったあと、
 * キーを離すまでの repeat が送信を繰り返さないようにする
 */
export const ignoreRepeatedModEnter = Prec.high(
  EditorView.domEventHandlers({
    keydown(event) {
      if (event.key !== 'Enter' || !(event.ctrlKey || event.metaKey) || !event.repeat) {
        return false;
      }
      event.preventDefault();
      return true;
    },
  }),
);
