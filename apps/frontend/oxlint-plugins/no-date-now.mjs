const noDateNow = {
  meta: {
    type: 'problem',
    docs: {
      description:
        'disallow Date.now() in application source (use new Date() or performance.now())',
    },
    schema: [],
    messages: {
      noDateNow:
        'Date.now() is intentionally not frozen in Storybook. Use new Date() for wall-clock time or performance.now() for elapsed time.',
    },
  },
  create(context) {
    return {
      CallExpression(node) {
        if (
          node.callee?.type === 'MemberExpression' &&
          !node.callee.computed &&
          node.callee.object?.type === 'Identifier' &&
          node.callee.object.name === 'Date' &&
          node.callee.property?.type === 'Identifier' &&
          node.callee.property.name === 'now'
        ) {
          context.report({ node, messageId: 'noDateNow' });
        }
      },
    };
  },
};

export default {
  meta: { name: 'no-date-now' },
  rules: {
    'no-date-now': noDateNow,
  },
};
