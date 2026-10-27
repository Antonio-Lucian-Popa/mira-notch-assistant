import { NativeModules } from 'react-native';
import type { Snapshot } from './model';
type Bridge = {
  voiceModelDownload(): Promise<string>;
  voiceAuto(enabled: boolean): Promise<string>;
  voiceStart(dictate: boolean): Promise<string>;
  voiceStop(): Promise<string>;
  transcript(id: string): Promise<string>;
  deleteTranscript(id: string): Promise<string>;
  sharePdf(id: string): Promise<string>;
  pair(code: string): Promise<string>;
  unpair(): Promise<string>;
  syncNow(): Promise<string>;
  remoteActions(): Promise<string>;
  remoteAction(id: string): Promise<string>;
  shake(enabled: boolean): Promise<string>;
  scanPairing(): Promise<string>;
  snapshot(): Promise<string>;
  action(action: string, payload: string): Promise<string>;
  overlayPermission(): Promise<string>;
  notificationSettings(): Promise<string>;
  overlay(enabled: boolean): Promise<string>;
  preview(expression: string): Promise<string>;
};
const bridge = NativeModules.MiraCompanion as Bridge;
export const companion = {
  async voiceModelDownload() {
    await bridge.voiceModelDownload();
  },
  async voiceAuto(enabled: boolean) {
    await bridge.voiceAuto(enabled);
  },
  async voiceStart(dictate: boolean) {
    await bridge.voiceStart(dictate);
  },
  async voiceStop() {
    await bridge.voiceStop();
  },
  async transcript(id: string) {
    return JSON.parse(await bridge.transcript(id));
  },
  async deleteTranscript(id: string) {
    await bridge.deleteTranscript(id);
  },
  async sharePdf(id: string) {
    await bridge.sharePdf(id);
  },
  async pair(code: string) {
    await bridge.pair(code);
  },
  async unpair() {
    await bridge.unpair();
  },
  async syncNow() {
    await bridge.syncNow();
  },
  async remoteActions() {
    return JSON.parse(await bridge.remoteActions());
  },
  async shake(enabled: boolean) {
    await bridge.shake(enabled);
  },
  async remoteAction(id: string) {
    await bridge.remoteAction(id);
  },
  async scanPairing() {
    return bridge.scanPairing();
  },
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
