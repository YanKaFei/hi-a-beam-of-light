import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeObserve } from '../utils/observe.js';

test('network failures are not reported as an empty device list', async () => {
  const ok = async () => ({ status: 200, json: {} });
  const controller = makeObserve({ api: { me: ok, myDevices: async () => ({ status: 0, json: {} }), myObservations: ok } });
  const steps = [];
  const result = await controller.run({ onStep: (step) => steps.push(step), timeoutMs: 30 });
  assert.equal(result.error, 'ERROR');
  assert.equal(steps.at(-1).stage, 'ERROR');
  assert.equal(steps.some((step) => step.stage === 'DISCOVERING_CONNECTIONS'), false);
});
