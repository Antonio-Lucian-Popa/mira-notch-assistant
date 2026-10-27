/** Versioned data contract for the future authenticated desktop bridge.
 * No transport, pairing UI, credentials or network listener is enabled in this preview.
 * IDs are scoped by origin; a completed recurring occurrence is identified by due.
 */
export type SyncEnvelope = {
  protocol: 'mira-companion/1';
  eventId: string;
  deviceId: string;
  revision: number;
  sentAt: number;
  change:
    | {
        kind: 'reminder.upsert';
        id: string;
        title: string;
        due: number;
        daily: boolean;
      }
    | { kind: 'reminder.complete'; id: string; expectedDue: number }
    | { kind: 'reminder.delete'; id: string }
    | {
        kind: 'focus.state';
        id: string;
        due: number;
        paused: boolean;
        remaining: number;
      };
};
