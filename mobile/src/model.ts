export type Theme = 'classic' | 'robot' | 'cat' | 'pixel';
export type Reminder = {
  id: string;
  title: string;
  due: number;
  daily: boolean;
  delivered?: boolean;
};
export type Focus = {
  id: string;
  due: number;
  remaining: number;
  paused: boolean;
};
export type Snapshot = {
  appVersion?: string;
  reminders: Reminder[];
  theme: Theme;
  scale: number;
  offset: number;
  motion?: boolean;
  cameraMode?: boolean;
  cameraX?: number;
  cameraY?: number;
  voiceModel?: {
    ready: boolean;
    downloading: boolean;
    progress: number;
    bytes: number;
    message: string;
  };
  voiceEnabled?: boolean;
  voice?: {
    active: boolean;
    state: string;
    message: string;
    partial?: string;
    transcriptId?: string;
  };
  transcripts?: Transcript[];
  peer?: { paired: boolean; message: string };
  today: { water?: number; completed?: number; sessions?: number };
  focus?: Focus;
  overlayGranted?: boolean;
  overlayRunning?: boolean;
  shakeEnabled?: boolean;
  notificationsGranted?: boolean;
};
export const initialState: Snapshot = {
  reminders: [],
  theme: 'classic',
  scale: 1,
  offset: 8,
  today: {},
};
export const themes: {
  id: Theme;
  name: string;
  color: string;
  detail: string;
}[] = [
  {
    id: 'classic',
    name: 'Mira',
    color: '#82deee',
    detail: 'Calmă și curioasă',
  },
  { id: 'robot', name: 'Orbit', color: '#85efb7', detail: 'Micul tău robot' },
  { id: 'cat', name: 'Nori', color: '#ffcd82', detail: 'Un strop de joacă' },
  { id: 'pixel', name: 'Pixel', color: '#bea4ff', detail: 'Cu un aer retro' },
];
export function duration(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  if (seconds >= 3600)
    return `${Math.floor(seconds / 3600)}h ${Math.floor(
      (seconds % 3600) / 60,
    )}m`;
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}
export function urgency(
  reminders: Reminder[],
  now: number,
): Reminder | undefined {
  return [...reminders]
    .sort((a, b) => a.due - b.due)
    .find(r => r.due - now <= 60_000);
}
export function reminderDraft(
  title: string,
  minutes: string,
  clock: string,
  daily: boolean,
  now = Date.now(),
) {
  title = title.trim();
  if (!title || title.length > 160)
    throw new Error('Scrie un titlu de cel mult 160 de caractere.');
  let due: number;
  if (clock.trim()) {
    const match = /^(\d{1,2}):(\d{2})$/.exec(clock.trim());
    if (!match || Number(match[1]) > 23 || Number(match[2]) > 59)
      throw new Error('Scrie ora în format HH:MM, de exemplu 13:00.');
    const date = new Date(now);
    date.setHours(Number(match[1]), Number(match[2]), 0, 0);
    if (date.getTime() <= now) date.setDate(date.getDate() + 1);
    due = date.getTime();
  } else {
    const n = Number(minutes);
    if (!/^\d+$/.test(minutes) || n < 1 || n > 10080)
      throw new Error('Alege între 1 și 10080 de minute sau scrie o oră.');
    due = now + n * 60000;
  }
  return { title, due, daily };
}

export type Transcript = {
  id: string;
  createdAt: number;
  title: string;
  complete: boolean;
  synced: boolean;
  preview?: string;
  text?: string;
  characters?: number;
};
export type RemoteAction = {
  id: string;
  name: string;
  keys: string[];
  lock: boolean;
};
