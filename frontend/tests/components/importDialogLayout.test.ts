import test from 'node:test';
import assert from 'node:assert/strict';

import {
  getImportAudioDialogLayoutClasses,
  type ImportAudioDialogLayoutClasses,
} from '../../src/components/ImportAudio/importDialogLayout.ts';

test('dialog content uses viewport-safe width and hides overflow', () => {
  const classes = getImportAudioDialogLayoutClasses();

  assert.equal(classes.dialogContentClassName, 'w-[calc(100vw-2rem)] max-w-[500px] overflow-hidden');
});

test('content wrapper allows the layout to shrink without breaking the footer', () => {
  const classes = getImportAudioDialogLayoutClasses();

  assert.equal(classes.contentClassName, 'space-y-4 py-4 max-w-full');
});

test('filename row uses truncation styles so long names do not overflow', () => {
  const classes = getImportAudioDialogLayoutClasses();

  assert.equal(
    classes.filenameClassName,
    'font-medium text-gray-900 truncate w-full overflow-hidden text-ellipsis whitespace-nowrap'
  );
});

test('title input and footer remain safe for narrow layouts', () => {
  const classes = getImportAudioDialogLayoutClasses();

  assert.equal(classes.titleInputClassName, 'w-full min-w-0');
  assert.equal(classes.footerClassName, 'flex-shrink-0');
});

test('the layout helper returns a stable class bundle object', () => {
  const classes: ImportAudioDialogLayoutClasses = getImportAudioDialogLayoutClasses();

  assert.deepEqual(classes, {
    dialogContentClassName: 'w-[calc(100vw-2rem)] max-w-[500px] overflow-hidden',
    contentClassName: 'space-y-4 py-4 max-w-full',
    filenameClassName: 'font-medium text-gray-900 truncate w-full overflow-hidden text-ellipsis whitespace-nowrap',
    titleInputClassName: 'w-full min-w-0',
    footerClassName: 'flex-shrink-0',
  });
});
