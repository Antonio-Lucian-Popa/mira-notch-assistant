import React from 'react';
import { act, create, ReactTestRenderer } from 'react-test-renderer';
import { PermissionsAndroid } from 'react-native';
import { VoicePanel } from '../src/VoicePanel';
import { initialState } from '../src/model';
import { companion } from '../src/native';
jest.mock('../src/Eyes', () => ({ Eyes: () => null }));
jest.mock('../src/native', () => ({
  companion: {
    voiceAuto: jest.fn().mockResolvedValue(undefined),
    voiceModelDownload: jest.fn().mockResolvedValue(undefined),
  },
}));
const withModel = {
  ...initialState,
  voiceModel: {
    ready: true,
    downloading: false,
    progress: 100,
    bytes: 147951465,
    message: 'Ready',
  },
};
const run = async (fn: () => Promise<void>) => {
  await fn();
  return true;
};
afterEach(() => {
  jest.restoreAllMocks();
  jest.clearAllMocks();
});
it('enables persistent activation only after microphone permission', async () => {
  jest
    .spyOn(PermissionsAndroid, 'request')
    .mockResolvedValue(PermissionsAndroid.RESULTS.GRANTED);
  let app!: ReactTestRenderer;
  await act(async () => {
    app = create(<VoicePanel data={withModel} run={run} />);
  });
  const toggle = app.root.findAll(
    p =>
      p.props.accessibilityLabel === 'Activare vocală Mira' &&
      typeof p.props.onValueChange === 'function',
  )[0];
  await act(async () => {
    toggle.props.onValueChange(true);
  });
  expect(PermissionsAndroid.request).toHaveBeenCalledWith(
    PermissionsAndroid.PERMISSIONS.RECORD_AUDIO,
  );
  expect(companion.voiceAuto).toHaveBeenCalledWith(true);
  await act(async () => app.unmount());
});
it('disables persistent activation without requesting microphone permission', async () => {
  const permission = jest.spyOn(PermissionsAndroid, 'request');
  let app!: ReactTestRenderer;
  await act(async () => {
    app = create(
      <VoicePanel data={{ ...withModel, voiceEnabled: true }} run={run} />,
    );
  });
  const toggle = app.root.findAll(
    p =>
      p.props.accessibilityLabel === 'Activare vocală Mira' &&
      typeof p.props.onValueChange === 'function',
  )[0];
  await act(async () => {
    toggle.props.onValueChange(false);
  });
  expect(companion.voiceAuto).toHaveBeenCalledWith(false);
  expect(permission).not.toHaveBeenCalled();
  await act(async () => app.unmount());
});
it('does not activate the microphone after permission is denied', async () => {
  jest
    .spyOn(PermissionsAndroid, 'request')
    .mockResolvedValue(PermissionsAndroid.RESULTS.DENIED);
  const failure = jest.fn();
  const guardedRun = async (fn: () => Promise<void>) => {
    try {
      await fn();
      return true;
    } catch (error) {
      failure(error);
      return false;
    }
  };
  let app!: ReactTestRenderer;
  await act(async () => {
    app = create(<VoicePanel data={withModel} run={guardedRun} />);
  });
  const toggle = app.root.findAll(
    p =>
      p.props.accessibilityLabel === 'Activare vocală Mira' &&
      typeof p.props.onValueChange === 'function',
  )[0];
  await act(async () => {
    toggle.props.onValueChange(true);
  });
  expect(companion.voiceAuto).not.toHaveBeenCalled();
  expect(failure).toHaveBeenCalled();
  await act(async () => app.unmount());
});
it('requires a one-time model download without opening the microphone', async () => {
  const permission = jest.spyOn(PermissionsAndroid, 'request');
  let app!: ReactTestRenderer;
  await act(async () => {
    app = create(<VoicePanel data={initialState} run={run} />);
  });
  const toggle = app.root.findAll(
    p =>
      p.props.accessibilityLabel === 'Activare vocală Mira' &&
      typeof p.props.onValueChange === 'function',
  )[0];
  expect(toggle.props.disabled).toBe(true);
  const download = app.root.findAll(
    p =>
      p.props.accessibilityLabel === 'Descarcă modelul gratuit · 148 MB' &&
      typeof p.props.onPress === 'function',
  )[0];
  await act(async () => {
    download.props.onPress();
  });
  expect(companion.voiceModelDownload).toHaveBeenCalled();
  expect(permission).not.toHaveBeenCalled();
  expect(companion.voiceAuto).not.toHaveBeenCalled();
  await act(async () => app.unmount());
});
