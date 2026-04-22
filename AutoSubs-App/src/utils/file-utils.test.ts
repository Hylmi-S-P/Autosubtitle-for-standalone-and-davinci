import { describe, it, expect } from 'vitest';
import { generateTranscriptTxt } from './file-utils';
import { Subtitle, Speaker } from '@/types/interfaces';

describe('generateTranscriptTxt', () => {
  it('should return empty string for empty subtitles array', () => {
    expect(generateTranscriptTxt([])).toBe('');
  });

  it('should normalize multiple spaces into single space and join without speakers', () => {
    const subtitles: Subtitle[] = [
      { id: 1, start: 0, end: 1, text: 'Hello  world. ', words: [] },
      { id: 2, start: 1, end: 2, text: ' How are    you? ', words: [] }
    ];
    expect(generateTranscriptTxt(subtitles)).toBe('Hello world. How are you?');
  });

  it('should group subtitles with the same speaker consecutively', () => {
    const subtitles: Subtitle[] = [
      { id: 1, start: 0, end: 1, text: 'Hello.', speaker_id: '1', words: [] },
      { id: 2, start: 1, end: 2, text: 'How are you?', speaker_id: '1', words: [] }
    ];
    const expected = `Speaker 1:\nHello. How are you?`;
    expect(generateTranscriptTxt(subtitles)).toBe(expected);
  });

  it('should format alternating speakers properly', () => {
    const subtitles: Subtitle[] = [
      { id: 1, start: 0, end: 1, text: 'Hello.', speaker_id: '1', words: [] },
      { id: 2, start: 1, end: 2, text: 'Hi there!', speaker_id: '2', words: [] },
      { id: 3, start: 2, end: 3, text: 'How are you?', speaker_id: '1', words: [] }
    ];
    const expected = `Speaker 1:\nHello.\n\nSpeaker 2:\nHi there!\n\nSpeaker 1:\nHow are you?`;
    expect(generateTranscriptTxt(subtitles)).toBe(expected);
  });

  it('should fallback to 0-based indexing if a speaker_id is "0"', () => {
    const subtitles: Subtitle[] = [
      { id: 1, start: 0, end: 1, text: 'Hey.', speaker_id: '0', words: [] },
      { id: 2, start: 1, end: 2, text: 'Yes?', speaker_id: '1', words: [] }
    ];
    const expected = `Speaker 0:\nHey.\n\nSpeaker 1:\nYes?`;
    expect(generateTranscriptTxt(subtitles)).toBe(expected);
  });

  it('should resolve speaker names using the speakers array (1-based fallback)', () => {
    const subtitles: Subtitle[] = [
      { id: 1, start: 0, end: 1, text: 'Hey.', speaker_id: '1', words: [] },
      { id: 2, start: 1, end: 2, text: 'Yes?', speaker_id: '2', words: [] }
    ];
    const speakers: Speaker[] = [
      { name: 'Alice', style: 'Fill', color: '#fff', sample: { start: 0, end: 1 } },
      { name: 'Bob', style: 'Fill', color: '#fff', sample: { start: 1, end: 2 } }
    ];
    const expected = `Alice:\nHey.\n\nBob:\nYes?`;
    expect(generateTranscriptTxt(subtitles, speakers)).toBe(expected);
  });

  it('should resolve speaker names using the speakers array (0-based fallback)', () => {
    const subtitles: Subtitle[] = [
      { id: 1, start: 0, end: 1, text: 'Hey.', speaker_id: '0', words: [] },
      { id: 2, start: 1, end: 2, text: 'Yes?', speaker_id: '1', words: [] }
    ];
    const speakers: Speaker[] = [
      { name: 'Alice', style: 'Fill', color: '#fff', sample: { start: 0, end: 1 } },
      { name: 'Bob', style: 'Fill', color: '#fff', sample: { start: 1, end: 2 } }
    ];
    const expected = `Alice:\nHey.\n\nBob:\nYes?`;
    expect(generateTranscriptTxt(subtitles, speakers)).toBe(expected);
  });

  it('should handle undefined speaker names correctly', () => {
    const subtitles: Subtitle[] = [
      { id: 1, start: 0, end: 1, text: 'Hey.', speaker_id: '1', words: [] }
    ];
    const speakers: Speaker[] = [
      { name: '  ', style: 'Fill', color: '#fff', sample: { start: 0, end: 1 } }
    ];
    // Since name is empty, fallback to Speaker 1
    const expected = `Speaker 1:\nHey.`;
    expect(generateTranscriptTxt(subtitles, speakers)).toBe(expected);
  });

  it('should use "Transcript" as speakerLabel if no speaker_id is provided but other segments have speakers', () => {
    const subtitles: Subtitle[] = [
      { id: 1, start: 0, end: 1, text: 'Intro text.', words: [] },
      { id: 2, start: 1, end: 2, text: 'First speaker speaking.', speaker_id: '1', words: [] }
    ];
    const expected = `Transcript:\nIntro text.\n\nSpeaker 1:\nFirst speaker speaking.`;
    expect(generateTranscriptTxt(subtitles)).toBe(expected);
  });

  it('should skip subtitles with empty text after normalization', () => {
    const subtitles: Subtitle[] = [
      { id: 1, start: 0, end: 1, text: '   ', speaker_id: '1', words: [] },
      { id: 2, start: 1, end: 2, text: 'Valid text.', speaker_id: '1', words: [] }
    ];
    const expected = `Speaker 1:\nValid text.`;
    expect(generateTranscriptTxt(subtitles)).toBe(expected);
  });
});
