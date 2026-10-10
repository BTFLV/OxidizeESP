import { describe, expect, it } from 'vitest';
import type { DeviceSnapshot } from '$lib/bindings/DeviceSnapshot';
import { deviceBlocker, statusLabel } from './device';
import { choosePreferredPort } from './stores/devices.svelte';

const port = { name: 'COM3', kind: 'usb', usb: null, hint: 'usbUartBridge', bridge: null } as const;

function device(overrides: Partial<DeviceSnapshot> = {}): DeviceSnapshot {
  return {
    id: 'd',
    port,
    status: 'unverified',
    activity: { type: 'idle' },
    chipInfo: null,
    verifiedAt: null,
    usbIdentity: null,
    previousPortName: null,
    lastError: null,
    ...overrides,
  };
}

describe('deviceBlocker', () => {
  it('explains why an operation cannot start', () => {
    expect(deviceBlocker(null)).toMatch(/Connect/);
    expect(deviceBlocker(device({ status: 'disconnected' }))).toMatch(/disconnected/);
    expect(deviceBlocker(device({ status: 'busy', activity: { type: 'monitoring', monitorId: 'm' } }))).toMatch(/serial monitor/);
    expect(deviceBlocker(device())).toMatch(/Identify/);
    expect(deviceBlocker(device(), false)).toBeNull();
  });

  it('labels busy devices by activity', () => {
    expect(statusLabel(device({ status: 'busy', activity: { type: 'operation', jobId: 'j', kind: 'flashElf' } }))).toBe('Flashing');
  });
});

describe('choosePreferredPort', () => {
  const ports = [port, { ...port, name: 'COM4', hint: 'other' as const }];
  it('remembers the last port when present', () => {
    expect(choosePreferredPort(ports, 'rememberLast', 'COM4')).toBe('COM4');
  });
  it('falls back to a single likely ESP port', () => {
    expect(choosePreferredPort(ports, 'rememberLast', 'COM9')).toBe('COM3');
    expect(choosePreferredPort(ports, 'autoSelectSingle', null)).toBe('COM3');
  });
  it('never preselects in manual mode', () => {
    expect(choosePreferredPort(ports, 'manual', 'COM3')).toBeNull();
  });
});
