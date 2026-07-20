import { describe, expect, it, vi } from 'vitest';
import plugin from './no-date-now.mjs';

const rule = plugin.rules['no-date-now'];

function callExpression(objectName, propertyName) {
  return {
    type: 'CallExpression',
    callee: {
      type: 'MemberExpression',
      computed: false,
      object: { type: 'Identifier', name: objectName },
      property: { type: 'Identifier', name: propertyName },
    },
  };
}

function runVisitor(node) {
  const report = vi.fn();
  const visitor = rule.create({ report });
  visitor.CallExpression(node);
  return report;
}

describe('no-date-now rule', () => {
  it('reports Date.now()', () => {
    const report = runVisitor(callExpression('Date', 'now'));
    expect(report).toHaveBeenCalledOnce();
    expect(report).toHaveBeenCalledWith({
      node: callExpression('Date', 'now'),
      messageId: 'noDateNow',
    });
  });

  it('does not report foo.now()', () => {
    const report = runVisitor(callExpression('foo', 'now'));
    expect(report).not.toHaveBeenCalled();
  });

  it('does not report Date.parse()', () => {
    const report = runVisitor(callExpression('Date', 'parse'));
    expect(report).not.toHaveBeenCalled();
  });
});
