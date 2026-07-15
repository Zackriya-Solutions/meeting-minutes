import assert from 'node:assert/strict';
import { standupMetrics } from './quality-gate.mjs';

function test(name, check) {
  check();
  process.stdout.write(`ok - ${name}\n`);
}

test('split protocol rejects one series leaking across train and test', () => {
  const metrics = standupMetrics([
    { id: 'one', series_id: 'daily-team', split: 'train', provider: 'deepseek', schema_version: 'v2', prompt_version: 'p1', meeting_type: 'pure_status', success: false },
    { id: 'two', series_id: 'daily-team', split: 'test', provider: 'deepseek', schema_version: 'v2', prompt_version: 'p1', meeting_type: 'pure_status', success: false },
  ]);
  assert.equal(metrics.protocol_error_count, 1);
  assert.equal(metrics.reviewed_standup_count, 2);
});

test('record metrics penalize invented facts, duplicate actions, owners, and bad evidence', () => {
  const metrics = standupMetrics([{
    id: 'sample',
    series_id: 'daily-team',
    split: 'test',
    provider: 'deepseek',
    schema_version: 'v2',
    prompt_version: 'p1',
    meeting_type: 'pure_status',
    success: true,
    latency_ms: 10,
    valid_timestamps: ['[01:00]'],
    reference_records: [
      { id: 'a1', kind: 'action', owner: null },
    ],
    hypothesis_records: [
      { kind: 'action', match_id: 'a1', owner: 'Иван', evidence: [{ timestamp: '[09:99]' }] },
      { kind: 'action', match_id: 'a1', owner: null, evidence: [{ timestamp: '[01:00]' }] },
      { kind: 'decision', match_id: null, evidence: [] },
    ],
  }]);
  assert.equal(metrics.fact_coverage, 1);
  assert.equal(metrics.action_precision, 0.5);
  assert.equal(metrics.action_recall, 1);
  assert.equal(metrics.action_f1, 2 / 3);
  assert.equal(metrics.owner_precision_when_shown, 0);
  assert.equal(metrics.unsupported_decision_action_rate, 1 / 3);
  assert.equal(metrics.invalid_evidence_rate, 2 / 3);
  assert.equal(metrics.duplicate_output_rate, 1 / 3);
  assert.equal(metrics.reviewed_standup_count, 1);
  assert.equal(metrics.contrast_count, 0);
});

test('provider failures count against success without pretending to have quality labels', () => {
  const metrics = standupMetrics([
    { id: 'failed', series_id: 'daily-team', split: 'dev', provider: 'deepseek', schema_version: 'v2', prompt_version: 'p1', meeting_type: 'pure_status', success: false },
  ]);
  assert.equal(metrics.success_rate, 0);
  assert.equal(metrics.quality_count, 0);
  assert.equal(metrics.fact_coverage, null);
});

test('meeting type must be manually reviewed before the corpus is valid', () => {
  const metrics = standupMetrics([{
    id: 'unreviewed-type',
    series_id: 'daily-team',
    split: 'dev',
    provider: 'deepseek',
    schema_version: 'v2',
    prompt_version: 'p1',
    meeting_type: 'UNASSIGNED',
    success: false,
  }]);
  assert.equal(metrics.protocol_error_count, 1);
  assert.equal(metrics.uncertain_count, 0);
});

test('standups and contrast meetings are counted separately', () => {
  const common = { series_id: 'series', split: 'dev', provider: 'deepseek', schema_version: 'v2', prompt_version: 'p1', success: false };
  const metrics = standupMetrics([
    { ...common, id: 'status', meeting_type: 'status_plus_deep_dive' },
    { ...common, id: 'planning', meeting_type: 'planning_sync' },
    { ...common, id: 'uncertain', meeting_type: 'uncertain' },
  ]);
  assert.equal(metrics.reviewed_standup_count, 1);
  assert.equal(metrics.contrast_count, 1);
  assert.equal(metrics.uncertain_count, 1);
});
