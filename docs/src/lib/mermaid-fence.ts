const text = (node: any): string =>
  node.type === "text" ? node.value : (node.children ?? []).map(text).join("");

const isMermaid = (node: any) =>
  node?.tagName === "pre" && [node.properties?.dataLanguage, node.properties?.["data-language"]].includes("mermaid");

/** ```mermaid のコードブロックを、src/scripts/mermaid.ts が描く pre.mermaid に置き換える。 */
export const mermaidFence = () => ({
  name: "koyori:mermaid-fence",
  element: {
    filter: ["figure"],
    visit(node: any) {
      const pre = node.children?.find(isMermaid);
      if (!pre) return;
      return {
        type: "element" as const,
        tagName: "pre",
        properties: { className: ["mermaid"] },
        children: [{ type: "text" as const, value: text(pre) }],
      };
    },
  },
});
