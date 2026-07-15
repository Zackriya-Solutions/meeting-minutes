#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

function parseArgs(argv) {
  const args = { mode: 'release' };
  for (let i = 0; i < argv.length; i += 1) {
    const value = argv[i];
    if (!value.startsWith('--')) throw new Error(`unexpected argument: ${value}`);
    const key = value.slice(2);
    args[key] = argv[i + 1];
    i += 1;
  }
  return args;
}

function loadJson(file, label) {
  if (!file) throw new Error(`${label} path is required`);
  return JSON.parse(fs.readFileSync(path.resolve(file), 'utf8'));
}

function words(text) {
  return String(text ?? '').toLocaleLowerCase('ru-RU').match(/[\p{L}\p{N}]+/gu) ?? [];
}

function editDistance(left, right) {
  let previous = Array.from({ length: right.length + 1 }, (_, index) => index);
  for (let i = 1; i <= left.length; i += 1) {
    const current = [i];
    for (let j = 1; j <= right.length; j += 1) {
      current[j] = Math.min(
        current[j - 1] + 1,
        previous[j] + 1,
        previous[j - 1] + (left[i - 1] === right[j - 1] ? 0 : 1),
      );
    }
    previous = current;
  }
  return previous[right.length];
}

function transcriptionMetrics(rows) {
  let errors = 0;
  let referenceWords = 0;
  for (const row of rows) {
    const reference = words(row.reference);
    const hypothesis = words(row.hypothesis);
    errors += editDistance(reference, hypothesis);
    referenceWords += reference.length;
  }
  return {
    count: rows.length,
    word_errors: errors,
    reference_words: referenceWords,
    wer: referenceWords ? errors / referenceWords : null,
  };
}

function speakerAt(segments, timeMs) {
  const segment = segments.find((item) => item.start_ms <= timeMs && timeMs < item.end_ms);
  return segment?.speaker ?? null;
}

function bestSpeakerMap(overlap, hypothesisSpeakers, referenceSpeakers) {
  if (hypothesisSpeakers.length > 8 || referenceSpeakers.length > 8) {
    const pairs = [];
    for (const hyp of hypothesisSpeakers) for (const ref of referenceSpeakers) {
      pairs.push([overlap.get(`${hyp}\u0000${ref}`) ?? 0, hyp, ref]);
    }
    pairs.sort((a, b) => b[0] - a[0]);
    const usedHyp = new Set();
    const usedRef = new Set();
    const mapping = new Map();
    for (const [, hyp, ref] of pairs) {
      if (!usedHyp.has(hyp) && !usedRef.has(ref)) {
        mapping.set(hyp, ref); usedHyp.add(hyp); usedRef.add(ref);
      }
    }
    return mapping;
  }
  let best = { score: -1, mapping: new Map() };
  function visit(index, used, mapping, score) {
    if (index === hypothesisSpeakers.length) {
      if (score > best.score) best = { score, mapping: new Map(mapping) };
      return;
    }
    const hyp = hypothesisSpeakers[index];
    visit(index + 1, used, mapping, score);
    for (const ref of referenceSpeakers) {
      if (used.has(ref)) continue;
      used.add(ref); mapping.set(hyp, ref);
      visit(index + 1, used, mapping, score + (overlap.get(`${hyp}\u0000${ref}`) ?? 0));
      mapping.delete(hyp); used.delete(ref);
    }
  }
  visit(0, new Set(), new Map(), 0);
  return best.mapping;
}

function diarizationMetrics(rows, frameMs = 100) {
  let missed = 0;
  let falseAlarm = 0;
  let speakerError = 0;
  let referenceSpeech = 0;
  for (const row of rows) {
    const reference = row.reference ?? [];
    const hypothesis = row.hypothesis ?? [];
    const end = Math.max(0, ...reference.map((s) => s.end_ms), ...hypothesis.map((s) => s.end_ms));
    const frames = [];
    const overlap = new Map();
    for (let time = frameMs / 2; time < end; time += frameMs) {
      const ref = speakerAt(reference, time);
      const hyp = speakerAt(hypothesis, time);
      frames.push([ref, hyp]);
      if (ref && hyp) {
        const key = `${hyp}\u0000${ref}`;
        overlap.set(key, (overlap.get(key) ?? 0) + 1);
      }
    }
    const refSpeakers = [...new Set(reference.map((s) => s.speaker))];
    const hypSpeakers = [...new Set(hypothesis.map((s) => s.speaker))];
    const mapping = bestSpeakerMap(overlap, hypSpeakers, refSpeakers);
    for (const [ref, hyp] of frames) {
      if (ref) referenceSpeech += 1;
      if (ref && !hyp) missed += 1;
      else if (!ref && hyp) falseAlarm += 1;
      else if (ref && hyp && mapping.get(hyp) !== ref) speakerError += 1;
    }
  }
  return {
    count: rows.length,
    reference_speech_frames: referenceSpeech,
    missed_frames: missed,
    false_alarm_frames: falseAlarm,
    speaker_error_frames: speakerError,
    der: referenceSpeech ? (missed + falseAlarm + speakerError) / referenceSpeech : null,
  };
}

function retrievalMetrics(rows) {
  const answerable = rows.filter((row) => row.answerable !== false);
  const unanswerable = rows.filter((row) => row.answerable === false);
  let recall = 0;
  let reciprocalRank = 0;
  for (const row of answerable) {
    const expected = new Set(row.expected_chunk_ids ?? []);
    const returned = row.returned_chunk_ids ?? [];
    const found = returned.filter((id) => expected.has(id));
    recall += expected.size ? new Set(found).size / expected.size : 0;
    const first = returned.findIndex((id) => expected.has(id));
    reciprocalRank += first >= 0 ? 1 / (first + 1) : 0;
  }
  const falsePositives = unanswerable.filter((row) => row.found === true).length;
  return {
    count: rows.length,
    answerable_count: answerable.length,
    unanswerable_count: unanswerable.length,
    recall_at_k: answerable.length ? recall / answerable.length : null,
    mrr: answerable.length ? reciprocalRank / answerable.length : null,
    no_answer_false_positive_rate: unanswerable.length ? falsePositives / unanswerable.length : null,
  };
}

function percentile(values, percentileValue) {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(percentileValue * sorted.length) - 1)];
}

function mean(values) {
  return values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : null;
}

function summaryMetrics(rows) {
  const successes = rows.filter((row) => row.success === true);
  const qualityRows = successes.filter((row) => {
    const quality = row.quality;
    return quality
      && ['fact_coverage', 'unsupported_claim_rate', 'action_item_f1']
        .every((key) => Number.isFinite(quality[key]) && quality[key] >= 0 && quality[key] <= 1);
  });
  return {
    count: rows.length,
    success_rate: rows.length ? successes.length / rows.length : null,
    p95_latency_ms: percentile(successes.map((row) => row.latency_ms), 0.95),
    quality_count: qualityRows.length,
    mean_fact_coverage: mean(qualityRows.map((row) => row.quality.fact_coverage)),
    mean_unsupported_claim_rate: mean(
      qualityRows.map((row) => row.quality.unsupported_claim_rate),
    ),
    mean_action_item_f1: mean(qualityRows.map((row) => row.quality.action_item_f1)),
  };
}

function normalizedOptional(value) {
  const normalized = words(value).join(' ');
  return normalized || null;
}

function f1(precision, recall) {
  if (precision == null || recall == null) return null;
  return precision + recall ? (2 * precision * recall) / (precision + recall) : 0;
}

function standupMetrics(rows) {
  const meetingTypes = new Set([
    'pure_status',
    'status_plus_deep_dive',
    'planning_sync',
    'one_to_one',
    'general_meeting',
    'uncertain',
  ]);
  const reviewedStandupTypes = new Set(['pure_status', 'status_plus_deep_dive']);
  const contrastTypes = new Set(['planning_sync', 'one_to_one', 'general_meeting']);
  const successes = rows.filter((row) => row.success === true);
  const sampleIds = new Set();
  const seriesSplits = new Map();
  const providerRows = new Map();
  let protocolErrors = 0;
  let qualityCount = 0;
  let referenceRecords = 0;
  let coveredReferenceRecords = 0;
  let outputRecords = 0;
  let unsupportedOutputRecords = 0;
  let decisionActionOutputs = 0;
  let unsupportedDecisionActions = 0;
  let referenceActions = 0;
  let outputActions = 0;
  let matchedActions = 0;
  let shownOwners = 0;
  let correctOwners = 0;
  let evidenceItems = 0;
  let invalidEvidenceItems = 0;
  let recordsMissingEvidence = 0;
  let duplicateOutputs = 0;

  for (const row of rows) {
    if (!row.id || sampleIds.has(row.id)) protocolErrors += 1;
    if (row.id) sampleIds.add(row.id);
    if (!row.provider || row.provider === 'unknown'
      || !row.schema_version || row.schema_version === 'UNASSIGNED'
      || !row.prompt_version || row.prompt_version === 'UNASSIGNED') {
      protocolErrors += 1;
    }
    if (!meetingTypes.has(row.meeting_type)) protocolErrors += 1;
    if (row.provider && row.provider !== 'unknown') {
      const provider = providerRows.get(row.provider) ?? [];
      provider.push(row);
      providerRows.set(row.provider, provider);
    }
    if (!row.series_id || !['train', 'dev', 'test'].includes(row.split)) {
      protocolErrors += 1;
    } else {
      const splits = seriesSplits.get(row.series_id) ?? new Set();
      splits.add(row.split);
      seriesSplits.set(row.series_id, splits);
    }
  }
  for (const splits of seriesSplits.values()) {
    if (splits.size > 1) protocolErrors += 1;
  }

  for (const row of successes) {
    const references = new Map(
      (row.reference_records ?? [])
        .filter((item) => item?.id && item?.kind)
        .map((item) => [item.id, item]),
    );
    const outputs = (row.hypothesis_records ?? []).filter((item) => item?.kind);
    const validTimestamps = new Set(row.valid_timestamps ?? []);
    if (references.size > 0) qualityCount += 1;
    referenceRecords += references.size;
    outputRecords += outputs.length;
    referenceActions += [...references.values()].filter((item) => item.kind === 'action').length;
    outputActions += outputs.filter((item) => item.kind === 'action').length;

    const covered = new Set();
    const matchedActionIds = new Set();
    for (const output of outputs) {
      const reference = output.match_id ? references.get(output.match_id) : null;
      const validMatch = Boolean(reference && reference.kind === output.kind);
      if (!validMatch) unsupportedOutputRecords += 1;
      if (['action', 'decision'].includes(output.kind)) {
        decisionActionOutputs += 1;
        if (!validMatch) unsupportedDecisionActions += 1;
      }
      if (validMatch) {
        if (covered.has(reference.id)) duplicateOutputs += 1;
        covered.add(reference.id);
        if (output.kind === 'action') matchedActionIds.add(reference.id);
      }

      const evidence = Array.isArray(output.evidence) ? output.evidence : [];
      if (evidence.length === 0) recordsMissingEvidence += 1;
      for (const item of evidence) {
        evidenceItems += 1;
        if (!validTimestamps.has(item?.timestamp)) invalidEvidenceItems += 1;
      }

      const owner = normalizedOptional(output.owner);
      if (owner) {
        shownOwners += 1;
        if (validMatch && owner === normalizedOptional(reference.owner)) correctOwners += 1;
      }
    }
    coveredReferenceRecords += covered.size;
    matchedActions += matchedActionIds.size;
  }

  const actionPrecision = outputActions ? matchedActions / outputActions : null;
  const actionRecall = referenceActions ? matchedActions / referenceActions : null;
  const evidenceDenominator = evidenceItems + recordsMissingEvidence;
  const providers = Object.fromEntries(
    [...providerRows.entries()].sort(([left], [right]) => left.localeCompare(right)).map(
      ([provider, providerSamples]) => [provider, {
        count: providerSamples.length,
        success_rate: providerSamples.filter((row) => row.success === true).length
          / providerSamples.length,
        p95_latency_ms: percentile(
          providerSamples
            .filter((row) => row.success === true)
            .map((row) => row.latency_ms)
            .filter(Number.isFinite),
          0.95,
        ),
      }],
    ),
  );
  return {
    count: rows.length,
    reviewed_standup_count: rows.filter((row) => reviewedStandupTypes.has(row.meeting_type)).length,
    pure_status_count: rows.filter((row) => row.meeting_type === 'pure_status').length,
    status_plus_deep_dive_count: rows.filter(
      (row) => row.meeting_type === 'status_plus_deep_dive',
    ).length,
    contrast_count: rows.filter((row) => contrastTypes.has(row.meeting_type)).length,
    uncertain_count: rows.filter((row) => row.meeting_type === 'uncertain').length,
    train_count: rows.filter((row) => row.split === 'train').length,
    dev_count: rows.filter((row) => row.split === 'dev').length,
    test_count: rows.filter((row) => row.split === 'test').length,
    provider_count: providerRows.size,
    providers,
    success_rate: rows.length ? successes.length / rows.length : null,
    p95_latency_ms: percentile(
      successes.map((row) => row.latency_ms).filter(Number.isFinite),
      0.95,
    ),
    quality_count: qualityCount,
    protocol_error_count: protocolErrors,
    reference_record_count: referenceRecords,
    output_record_count: outputRecords,
    fact_coverage: referenceRecords ? coveredReferenceRecords / referenceRecords : null,
    unsupported_claim_rate: outputRecords ? unsupportedOutputRecords / outputRecords : null,
    unsupported_decision_action_rate: decisionActionOutputs
      ? unsupportedDecisionActions / decisionActionOutputs
      : null,
    action_precision: actionPrecision,
    action_recall: actionRecall,
    action_f1: f1(actionPrecision, actionRecall),
    owner_precision_when_shown: shownOwners ? correctOwners / shownOwners : null,
    invalid_evidence_rate: evidenceDenominator
      ? (invalidEvidenceItems + recordsMissingEvidence) / evidenceDenominator
      : null,
    duplicate_output_rate: outputRecords ? duplicateOutputs / outputRecords : null,
  };
}

function getMetric(metrics, dottedPath) {
  return dottedPath.split('.').reduce((value, key) => value?.[key], metrics);
}

function evaluate(metrics, thresholds) {
  const failures = [];
  for (const [section, minimum] of Object.entries(thresholds.minimum_counts ?? {})) {
    const actual = metrics[section]?.count ?? 0;
    if (actual < minimum) failures.push(`${section}.count=${actual} < required ${minimum}`);
  }
  for (const [metric, maximum] of Object.entries(thresholds.maximum ?? {})) {
    const actual = getMetric(metrics, metric);
    if (actual == null || actual > maximum) failures.push(`${metric}=${actual} > maximum ${maximum}`);
  }
  for (const [metric, minimum] of Object.entries(thresholds.minimum ?? {})) {
    const actual = getMetric(metrics, metric);
    if (actual == null || actual < minimum) failures.push(`${metric}=${actual} < minimum ${minimum}`);
  }
  return failures;
}

function runCli() {
  try {
    const args = parseArgs(process.argv.slice(2));
    const datasetPath = args.dataset ?? process.env.MEMENTO_QUALITY_DATASET;
    if (!datasetPath) {
      throw new Error('dataset is required; pass --dataset or set MEMENTO_QUALITY_DATASET');
    }
    const thresholdsPath = args.thresholds ?? `evals/thresholds.${args.mode}.json`;
    const dataset = loadJson(datasetPath, 'dataset');
    const thresholds = loadJson(thresholdsPath, 'thresholds');
    const metrics = {
      transcription: transcriptionMetrics(dataset.transcription ?? []),
      diarization: diarizationMetrics(dataset.diarization ?? [], dataset.frame_ms ?? 100),
      retrieval: retrievalMetrics(dataset.retrieval ?? []),
      summary: summaryMetrics(dataset.summary ?? []),
      standup: standupMetrics(dataset.standup ?? []),
    };
    const failures = evaluate(metrics, thresholds);
    const report = {
      version: 1,
      mode: args.mode,
      dataset_id: dataset.dataset_id ?? path.basename(datasetPath),
      generated_at: new Date().toISOString(),
      passed: failures.length === 0,
      metrics,
      thresholds,
      failures,
    };
    if (args.report) {
      fs.writeFileSync(path.resolve(args.report), `${JSON.stringify(report, null, 2)}\n`);
    }
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    process.exitCode = failures.length ? 1 : 0;
  } catch (error) {
    process.stderr.write(`quality gate error: ${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 2;
  }
}

const isMain = process.argv[1]
  && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) runCli();

export { evaluate, runCli, standupMetrics };
