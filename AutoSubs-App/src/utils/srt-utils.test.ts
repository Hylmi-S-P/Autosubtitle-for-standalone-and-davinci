import test from 'node:test';
import assert from 'node:assert';
import { formatTimecode, generateSrt } from './srt-utils.ts';

test('formatTimecode', () => {
    // 0 seconds
    assert.strictEqual(formatTimecode(0), '00:00:00,000');

    // whole seconds
    assert.strictEqual(formatTimecode(1), '00:00:01,000');
    assert.strictEqual(formatTimecode(59), '00:00:59,000');

    // minutes
    assert.strictEqual(formatTimecode(60), '00:01:00,000');
    assert.strictEqual(formatTimecode(61), '00:01:01,000');

    // hours
    assert.strictEqual(formatTimecode(3600), '01:00:00,000');
    assert.strictEqual(formatTimecode(3661), '01:01:01,000');
    assert.strictEqual(formatTimecode(36000), '10:00:00,000');

    // fractional seconds
    assert.strictEqual(formatTimecode(1.5), '00:00:01,500');
    assert.strictEqual(formatTimecode(1.001), '00:00:01,001');
    assert.strictEqual(formatTimecode(1.014), '00:00:01,014');

    // floating-point precision bug cases
    assert.strictEqual(formatTimecode(3661.999), '01:01:01,999');
    assert.strictEqual(formatTimecode(0.456), '00:00:00,456');
    assert.strictEqual(formatTimecode(0.999), '00:00:00,999');
});

test('generateSrt', () => {
    // Valid subtitles
    const subtitles = [
        { id: 1, start: 0.5, end: 2.5, text: 'Hello', words: [] },
        { id: 2, start: 3.0, end: 4.5, text: 'World', words: [] }
    ];

    const expected = "1\n00:00:00,500 --> 00:00:02,500\nHello\n\n2\n00:00:03,000 --> 00:00:04,500\nWorld\n";
    assert.strictEqual(generateSrt(subtitles), expected);

    // Empty subtitles
    assert.strictEqual(generateSrt([]), "");

    // Invalid timestamp skips subtitle
    const invalidSubtitles = [
        { id: 1, start: 0.5, end: 2.5, text: 'Hello', words: [] },
        { id: 2, start: NaN, end: 4.5, text: 'Invalid', words: [] }
    ];
    const expectedInvalid = "1\n00:00:00,500 --> 00:00:02,500\nHello\n";
    assert.strictEqual(generateSrt(invalidSubtitles), expectedInvalid);

    // Undefined text
    const noTextSubtitles = [
        { id: 1, start: 0.5, end: 2.5, text: undefined as any, words: [] }
    ];
    const expectedNoText = "1\n00:00:00,500 --> 00:00:02,500\n\n";
    assert.strictEqual(generateSrt(noTextSubtitles), expectedNoText);

    // Invalid input type
    assert.throws(() => generateSrt(null as any), /Subtitles must be an array/);
    assert.throws(() => generateSrt(undefined as any), /Subtitles must be an array/);
});
