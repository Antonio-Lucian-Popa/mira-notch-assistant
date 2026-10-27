import React from 'react';
import { act, create, ReactTestRenderer } from 'react-test-renderer';
import { TextInput } from 'react-native';
import App from '../App';
import { companion } from '../src/native';
import { initialState } from '../src/model';
jest.mock('react-native-safe-area-context', () => ({
  ...require('react-native-safe-area-context/jest/mock').default,
  SafeAreaView: require('react-native').View,
}));
jest.mock('../src/native', () => ({
  companion: {
    snapshot: jest.fn(),
    action: jest.fn().mockResolvedValue(undefined),
  },
}));
jest.mock('../src/Eyes', () => ({ Eyes: () => null }));
beforeEach(() => {
  jest.useFakeTimers();
  (companion.snapshot as jest.Mock).mockResolvedValue(initialState);
});
afterEach(() => {
  jest.useRealTimers();
  jest.clearAllMocks();
});
it('loads persisted state and creates a reminder from the actual form', async () => {
  let app!: ReactTestRenderer;
  await act(async () => {
    app = create(<App />);
  });
  const reminderTab = app.root.findAll(
    p =>
      p.props.accessibilityLabel === 'Remindere' &&
      typeof p.props.onPress === 'function',
  )[0];
  await act(async () => reminderTab.props.onPress());
  const title = app.root
    .findAllByType(TextInput)
    .find(i => i.props.accessibilityLabel === 'Titlu reminder')!;
  await act(async () => title.props.onChangeText('Pauză'));
  const add = app.root.findAll(
    p =>
      p.props.accessibilityLabel === 'Adaugă reminder' &&
      typeof p.props.onPress === 'function',
  )[0];
  await act(async () => add.props.onPress());
  expect(companion.action).toHaveBeenCalledWith(
    'add',
    expect.objectContaining({ title: 'Pauză', daily: false }),
  );
  await act(async () => app.unmount());
});
