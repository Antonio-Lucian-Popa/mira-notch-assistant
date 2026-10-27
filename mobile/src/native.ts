import { NativeModules } from 'react-native';
import type { Snapshot } from './model';
type Bridge = {
  snapshot(): Promise<string>;
  action(action: string, payload: string): Promise<string>;
  overlayPermission(): Promise<string>;
  notificationSettings(): Promise<string>;
  overlay(enabled: boolean): Promise<string>;
  preview(expression: string): Promise<string>;
};
const bridge = NativeModules.MiraCompanion as Bridge;
export const companion = {
  async snapshot(): Promise<Snapshot> {
    return JSON.parse(await bridge.snapshot());
  },
  async action(action: string, payload: object = {}): Promise<void> {
    await bridge.action(action, JSON.stringify(payload));
  },
  async notificationSettings() {
    await bridge.notificationSettings();
  },
  async permission() {
    await bridge.overlayPermission();
  },
  async overlay(enabled: boolean) {
    await bridge.overlay(enabled);
  },
  async preview(expression: string) {
    await bridge.preview(expression);
  },
};
