/**
 * koyori の Picker を試験で引く。トリガーは button、項目は option。一覧はトリガーと同じ根の中に
 * 描かれ、閉じている間も hidden のまま DOM にあるので、開かずに項目を引ける。
 */

/** 支援技術に読まれる字（aria-hidden を除く）。項目ではアバターの頭文字や補足を除いた label になる */
export function accessibleText(el: Element): string {
  return [...el.childNodes]
    .map((node) =>
      node instanceof Element
        ? node.getAttribute('aria-hidden') === 'true'
          ? ''
          : accessibleText(node)
        : (node.textContent ?? ''),
    )
    .join('')
    .trim();
}

/** Field の外のトリガーを label で引く（読み上げ名は「label」か「label: 選んだ項目」） */
export function pickerTrigger(root: ParentNode, label: string): HTMLButtonElement {
  const trigger = [...root.querySelectorAll<HTMLButtonElement>('button[aria-label]')].find(
    (button) => {
      const name = button.getAttribute('aria-label')!;
      return name === label || name.startsWith(`${label}: `);
    },
  );
  if (!trigger) throw new Error(`picker ${label} not found`);
  return trigger;
}

export function pickerOptions(trigger: Element): HTMLElement[] {
  return [...trigger.parentElement!.querySelectorAll<HTMLElement>('[role="option"]')];
}

/** 項目を名（label）で引く */
export function pickerOption(trigger: Element, name: string): HTMLElement {
  const option = pickerOptions(trigger).find((el) => accessibleText(el) === name);
  if (!option) throw new Error(`option ${name} not found`);
  return option;
}
