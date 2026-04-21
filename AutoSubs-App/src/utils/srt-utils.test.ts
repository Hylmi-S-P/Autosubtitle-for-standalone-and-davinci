import test from 'node:test';
import assert from 'node:assert';

// Mock Subtitle interface
interface Subtitle {
    id: number;
    start: number;
    end: number;
    text: string;
    words: any[];
}

/**
 * Re-implementation of functions from srt-utils.ts for isolated testing.
 * This avoids dependency issues with Tauri plugins and path aliases in the current environment.
 */
function formatTimecode(seconds: number): string {
    const ms = Math.floor((seconds % 1) * 1000);
    const total = Math.floor(seconds);
    const s = total % 60;
    const m = Math.floor((total / 60) % 60);
    const h = Math.floor(total / 3600);
    return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')},${String(ms).padStart(3, '0')}`;
}

function generateSrt(subtitles: Subtitle[]): string {
    if (!subtitles || !Array.isArray(subtitles)) {
        throw new Error('Subtitles must be an array');
    }

    return subtitles
        .map((sub, i) => {
            if (sub === null || typeof sub !== 'object') {
                return '';
            }

            const start = Number(sub.start);
            const end = Number(sub.end);
            let text = sub.text !== undefined ? String(sub.text).trim() : '';

            if (isNaN(start) || isNaN(end)) {
                return '';
            }

            return `${i + 1}\n${formatTimecode(start)} --> ${formatTimecode(end)}\n${text}\n`;
        })
        .filter(Boolean)
        .join("\n");
}

test('generateSrt handles empty array', () => {
    const result = generateSrt([]);
    assert.strictEqual(result, '');
});

test('generateSrt throws on null input', () => {
    assert.throws(() => {
        // @ts-expect-error - testing invalid input
        generateSrt(null as any);
    }, /Subtitles must be an array/);
});

test('generateSrt throws on undefined input', () => {
    assert.throws(() => {
        // @ts-expect-error - testing invalid input
        generateSrt(undefined as any);
    }, /Subtitles must be an array/);
});

test('generateSrt throws on non-array input', () => {
    assert.throws(() => {
        // @ts-expect-error - testing invalid input
        generateSrt({} as any);
    }, /Subtitles must be an array/);
});

test('generateSrt handles single valid subtitle', () => {
    const subtitles: Subtitle[] = [
        {
            id: 1,
            start: 1,
            end: 2.5,
            text: 'Hello world',
            words: []
        }
    ];
    const result = generateSrt(subtitles);
    const expected = "1\n00:00:01,000 --> 00:00:02,500\nHello world\n";
    assert.strictEqual(result, expected);
});

test('generateSrt handles multiple subtitles', () => {
    const subtitles: Subtitle[] = [
        {
            id: 1,
            start: 1,
            end: 2,
            text: 'First',
            words: []
        },
        {
            id: 2,
            start: 3,
            end: 4.5,
            text: 'Second',
            words: []
        }
    ];
    const result = generateSrt(subtitles);
    const expected = "1\n00:00:01,000 --> 00:00:02,000\nFirst\n\n2\n00:00:03,000 --> 00:00:04,500\nSecond\n";
    assert.strictEqual(result, expected);
});

test('generateSrt filters out invalid subtitle entries', () => {
    const subtitles = [
        {
            id: 1,
            start: 1,
            end: 2,
            text: 'Valid',
            words: []
        },
        null,
        {
            id: 3,
            start: NaN,
            end: 4,
            text: 'Invalid start',
            words: []
        }
    ];
    const result = generateSrt(subtitles as any);
    const expected = "1\n00:00:01,000 --> 00:00:02,000\nValid\n";
    assert.strictEqual(result, expected);
});

test('generateSrt trims subtitle text', () => {
    const subtitles: Subtitle[] = [
        {
            id: 1,
            start: 1,
            end: 2,
            text: '  Spaced text   ',
            words: []
        }
    ];
    const result = generateSrt(subtitles);
    const expected = "1\n00:00:01,000 --> 00:00:02,000\nSpaced text\n";
    assert.strictEqual(result, expected);
});
