import { describe, it, expect } from 'vitest';
import { formatTimecode, generateSrt } from './srt-utils.ts';

describe('formatTimecode', () => {
    it('handles 0 seconds', () => {
        expect(formatTimecode(0)).toBe('00:00:00,000');
    });

    it('handles whole seconds', () => {
        expect(formatTimecode(1)).toBe('00:00:01,000');
        expect(formatTimecode(59)).toBe('00:00:59,000');
    });

    it('handles minutes', () => {
        expect(formatTimecode(60)).toBe('00:01:00,000');
        expect(formatTimecode(61)).toBe('00:01:01,000');
    });

    it('handles hours', () => {
        expect(formatTimecode(3600)).toBe('01:00:00,000');
        expect(formatTimecode(3661)).toBe('01:01:01,000');
        expect(formatTimecode(36000)).toBe('10:00:00,000');
    });

    it('handles fractional seconds', () => {
        expect(formatTimecode(1.5)).toBe('00:00:01,500');
        expect(formatTimecode(1.001)).toBe('00:00:01,001');
        expect(formatTimecode(1.014)).toBe('00:00:01,014');
    });

    it('handles floating-point precision bug cases', () => {
        expect(formatTimecode(3661.999)).toBe('01:01:01,999');
        expect(formatTimecode(0.456)).toBe('00:00:00,456');
        expect(formatTimecode(0.999)).toBe('00:00:00,999');
    });
});

describe('generateSrt', () => {
    it('handles Valid subtitles', () => {
        const subtitles = [
            { id: 1, start: 0.5, end: 2.5, text: 'Hello', words: [] },
            { id: 2, start: 3.0, end: 4.5, text: 'World', words: [] }
        ];

        const expected = "1\n00:00:00,500 --> 00:00:02,500\nHello\n\n2\n00:00:03,000 --> 00:00:04,500\nWorld\n";
        expect(generateSrt(subtitles)).toBe(expected);
    });

    it('handles Empty subtitles', () => {
        expect(generateSrt([])).toBe("");
    });

    it('Invalid timestamp skips subtitle', () => {
        const invalidSubtitles = [
            { id: 1, start: 0.5, end: 2.5, text: 'Hello', words: [] },
            { id: 2, start: NaN, end: 4.5, text: 'Invalid', words: [] }
        ];
        const expectedInvalid = "1\n00:00:00,500 --> 00:00:02,500\nHello\n";
        expect(generateSrt(invalidSubtitles)).toBe(expectedInvalid);
    });

    it('handles Undefined text', () => {
        const noTextSubtitles = [
            { id: 1, start: 0.5, end: 2.5, text: undefined as any, words: [] }
        ];
        const expectedNoText = "1\n00:00:00,500 --> 00:00:02,500\n\n";
        expect(generateSrt(noTextSubtitles)).toBe(expectedNoText);
    });

    it('handles Invalid input type', () => {
        expect(() => generateSrt(null as any)).toThrow(/Subtitles must be an array/);
        expect(() => generateSrt(undefined as any)).toThrow(/Subtitles must be an array/);
    });
});
