import { describe, expect, it } from 'vitest';
import fixture from '../../tests/contracts/intelligence-wire.json';
import { WireDecodeError } from './wire';
import { decodeModelProfileView, decodeIntelligenceSettings, decodeManagedDataLocation, decodeJobPage, decodeIntelligenceEvent, decodeConnectionProbe, decodeIntelligenceErrorCode, intelligenceErrorSubcode } from './intelligence-wire';
describe('shared Rust intelligence contract', () => {
  it('preserves profiles, milliseconds, nullable fields, job states and event tags', () => {
    for (const value of fixture.profiles) expect(decodeModelProfileView(value)).toEqual(value);
    expect(decodeIntelligenceSettings(fixture.settings)).toEqual(fixture.settings);
    expect(decodeManagedDataLocation(fixture.managedDataLocation)).toEqual(fixture.managedDataLocation);
    expect(decodeJobPage(fixture.page)).toEqual(fixture.page);
    for (const value of fixture.events) expect(decodeIntelligenceEvent(value)).toEqual(value);
    for (const value of fixture.probes) expect(decodeConnectionProbe(value)).toEqual(value);
    for (const value of fixture.errorCodes) expect(decodeIntelligenceErrorCode(value)).toEqual(value);
  });
  it('rejects credential fields, malformed UUIDs, unbounded pages and unsafe numeric values', () => {
    const profile = fixture.profiles[0];
    expect(() => decodeModelProfileView({ ...profile, api_key: 'synthetic-leak' })).toThrow(WireDecodeError);
    expect(() => decodeModelProfileView({ ...profile, profile: { ...profile.profile, credential_reference: 'foreign' } })).toThrow(WireDecodeError);
    expect(() => decodeModelProfileView({ ...profile, profile: { ...profile.profile, id: 'bad' } })).toThrow(WireDecodeError);
    expect(() => decodeJobPage({ rows: Array(201).fill(fixture.page.rows[0]), next_cursor: null })).toThrow(WireDecodeError);
    for (const value of [NaN, Infinity, 1.5, Number.MAX_SAFE_INTEGER + 1]) expect(() => decodeConnectionProbe({ ...fixture.probes[0], checked_at_ms: value })).toThrow(WireDecodeError);
    expect(() => decodeIntelligenceEvent({ kind: 'job_changed', job: null })).toThrow(WireDecodeError);
  });
  it('decodes the stable two-bracket category without trusting remote prose', () => {
    for (const code of fixture.errorCodes) expect(intelligenceErrorSubcode(`[INTELLIGENCE_ERROR][${code}] Local message.`)).toBe(code);
    for (const value of ['[INTELLIGENCE_ERROR][unknown] body', '[NETWORK] body', '[INTELLIGENCE_ERROR] body', null]) expect(intelligenceErrorSubcode(value)).toBeNull();
  });
});
