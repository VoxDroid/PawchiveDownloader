import { describe, expect, it } from 'vitest';
import { describeError } from '../../src/ui/state/errors';

describe('describeError', () => {
  it('turns backend codes into readable text', () => {
    expect(describeError('not-found')).toBe('Pawchive does not have this page. Check the link.');
    expect(describeError('folder-unwritable: Access is denied. (os error 5)')).toMatch(
      /cannot be written to/,
    );
    expect(describeError('no connection')).toMatch(/waiting for the network/);
    expect(describeError('got a web page instead of the file')).toMatch(/web page/);
    expect(describeError('the file does not match its checksum')).toMatch(/checksum/);
    expect(describeError('internal error (task 7 panicked)')).toMatch(/report it/);
    expect(describeError('timed out')).toMatch(/too long/);
  });

  it('explains HTTP statuses', () => {
    expect(describeError('HTTP 404')).toBe('Not on the server (HTTP 404)');
    expect(describeError('HTTP 403')).toMatch(/denied/);
    expect(describeError('HTTP 429')).toMatch(/slow down/);
    expect(describeError('HTTP 502')).toMatch(/problem/);
    expect(describeError('HTTP 418')).toBe('The server answered HTTP 418');
  });

  it('keeps unknown messages and accepts errors of any kind', () => {
    expect(describeError('something new')).toBe('something new');
    expect(describeError(new Error('HTTP 404'))).toBe('Not on the server (HTTP 404)');
  });
});
