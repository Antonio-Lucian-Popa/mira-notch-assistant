import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  AppState,
  PermissionsAndroid,
  Platform,
  Pressable,
  ScrollView,
  StatusBar,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  View,
} from 'react-native';
import { SafeAreaProvider, SafeAreaView } from 'react-native-safe-area-context';
import { Eyes } from './src/Eyes';
import { NavIcon } from './src/NavIcon';
import { VoicePanel } from './src/VoicePanel';
import { companion } from './src/native';
import {
  duration,
  initialState,
  reminderDraft,
  themes,
  urgency,
  type Reminder,
  type Snapshot,
} from './src/model';

type Tab = 'today' | 'reminders' | 'studio' | 'voice' | 'settings';
const tabs: { id: Tab; title: string }[] = [
  { id: 'today', title: 'Acasă' },
  { id: 'reminders', title: 'Remindere' },
  { id: 'studio', title: 'Studio' },
  { id: 'voice', title: 'Voce & PC' },
  { id: 'settings', title: 'Setări' },
];
function Button({
  title,
  onPress,
  quiet = false,
  disabled = false,
}: {
  title: string;
  onPress: () => void;
  quiet?: boolean;
  disabled?: boolean;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={title}
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        s.button,
        quiet && s.quiet,
        { opacity: disabled ? 0.45 : pressed ? 0.7 : 1 },
      ]}
    >
      <Text style={[s.buttonText, quiet && s.quietText]}>{title}</Text>
    </Pressable>
  );
}
function Card({ children }: { children: React.ReactNode }) {
  return <View style={s.card}>{children}</View>;
}
function Caption({ children }: { children: React.ReactNode }) {
  return <Text style={s.caption}>{children}</Text>;
}
function MiraApp() {
  const [tab, setTab] = useState<Tab>('today');
  const [data, setData] = useState<Snapshot>(initialState);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [title, setTitle] = useState('');
  const [minutes, setMinutes] = useState('20');
  const [clock, setClock] = useState('');
  const [daily, setDaily] = useState(false);
  const [expression, setExpression] = useState('happy');
  const pending = useRef(false);
  const refresh = useCallback(async () => {
    try {
      setData(await companion.snapshot());
      setReady(true);
    } catch (e) {
      setError(String((e as Error).message));
    }
  }, []);
  useEffect(() => {
    refresh();
    let timer: ReturnType<typeof setInterval> | undefined;
    const tick = () => {
      setNow(Date.now());
      refresh();
    };
    const start = () => {
      if (timer) clearInterval(timer);
      timer = setInterval(tick, 1000);
    };
    start();
    const subscription = AppState.addEventListener('change', state => {
      if (state === 'active') {
        tick();
        start();
      } else if (timer) {
        clearInterval(timer);
        timer = undefined;
      }
    });
    return () => {
      subscription.remove();
      if (timer) clearInterval(timer);
    };
  }, [refresh]);
  const run = async (fn: () => Promise<void>): Promise<boolean> => {
    if (pending.current) return false;
    pending.current = true;
    setBusy(true);
    setError('');
    try {
      await fn();
      await refresh();
      return true;
    } catch (e) {
      setError((e as Error).message);
      return false;
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };
  const action = (name: string, payload: object = {}) =>
    run(() => companion.action(name, payload));
  const upcoming = [...data.reminders].sort((a, b) => a.due - b.due);
  const urgent = urgency(upcoming, now);
  const theme = themes.find(t => t.id === data.theme) ?? themes[0];
  const focus = data.focus;
  const remaining = focus
    ? focus.paused
      ? focus.remaining
      : focus.due - now
    : 25 * 60000;
  const toggleOverlay = () =>
    run(async () => {
      if (!data.overlayGranted) {
        await companion.permission();
        return;
      }
      if (!data.overlayRunning && Number(Platform.Version) >= 33) {
        await PermissionsAndroid.request(
          PermissionsAndroid.PERMISSIONS.POST_NOTIFICATIONS,
        );
      }
      await companion.overlay(!data.overlayRunning);
      setTimeout(refresh, 350);
    });
  const notifyPermission = () =>
    run(async () => {
      if (Number(Platform.Version) >= 33) {
        const status = await PermissionsAndroid.request(
          PermissionsAndroid.PERMISSIONS.POST_NOTIFICATIONS,
        );
        if (status === PermissionsAndroid.RESULTS.NEVER_ASK_AGAIN)
          await companion.notificationSettings();
      } else await companion.notificationSettings();
      await companion.action('settings');
    });
  const addReminder = () =>
    run(async () => {
      const draft = reminderDraft(title, minutes, clock, daily);
      await companion.action('add', draft);
      setTitle('');
      setClock('');
    });
  const deleteReminder = (r: Reminder) =>
    Alert.alert(
      'Ștergi reminderul?',
      r.daily ? 'Se va opri și repetarea zilnică.' : r.title,
      [
        { text: 'Păstrează', style: 'cancel' },
        {
          text: 'Șterge',
          style: 'destructive',
          onPress: () => action('delete', { id: r.id }),
        },
      ],
    );
  const titleOfPage = {
    today: 'În ritmul tău.',
    reminders: 'Nu uita de tine.',
    studio: 'Puțină personalitate.',
    voice: 'Spune „Mira”.',
    settings: 'Mira, pe gustul tău.',
  }[tab];
  return (
    <SafeAreaView style={s.root} edges={['top', 'bottom']}>
      <StatusBar barStyle="light-content" backgroundColor="#0b1016" />
      <View style={s.brandRow}>
        <View style={s.row}>
          <View style={s.brandDot} />
          <Text style={s.brand}>mira</Text>
        </View>
        <Text style={s.local}>COMPANIONUL TĂU</Text>
      </View>
      <ScrollView
        key={tab}
        showsVerticalScrollIndicator={false}
        contentContainerStyle={s.content}
        keyboardShouldPersistTaps="handled"
      >
        <Text style={s.eyebrow}>
          {tab === 'today'
            ? new Date(now).toLocaleDateString('ro-RO', {
                weekday: 'long',
                day: 'numeric',
                month: 'long',
              })
            : tabs.find(t => t.id === tab)!.title}
        </Text>
        <Text style={s.heading}>{titleOfPage}</Text>
        {!!error && (
          <Pressable onPress={() => setError('')} accessibilityRole="button">
            <Text accessibilityRole="alert" style={s.error}>
              {error} · Închide
            </Text>
          </Pressable>
        )}
        {!ready ? (
          <ActivityIndicator color="#85efb7" />
        ) : (
          <>
            {tab === 'today' && (
              <>
                <View style={[s.hero, urgent && s.urgentHero]}>
                  <View style={s.heroTop}>
                    <Text style={s.chip}>
                      {urgent
                        ? 'REMINDER APROPIAT'
                        : focus
                        ? 'TIMP PENTRU FOCUS'
                        : 'COMPANIONUL TĂU'}
                    </Text>
                    <View
                      style={[
                        s.dot,
                        urgent ? s.urgentDot : { backgroundColor: theme.color },
                      ]}
                    />
                  </View>
                  <Eyes
                    theme={data.theme}
                    compact
                    urgent={!!urgent}
                    motion={data.motion !== false}
                  />
                  <Text style={s.heroTitle}>
                    {urgent
                      ? urgent.title
                      : focus
                      ? 'Un singur lucru, pe rând.'
                      : 'Sunt aici. Tu cum ești?'}
                  </Text>
                  <Text style={s.heroText}>
                    {urgent
                      ? urgent.due <= now
                        ? 'E timpul. Un pas mic și ai rezolvat.'
                        : `Mai sunt ${duration(urgent.due - now)}. Ne pregătim?`
                      : 'Atinge și mișcă degetul peste ochi.'}
                  </Text>
                </View>
                <View style={s.sectionRow}>
                  <Text style={s.section}>Spațiu pentru concentrare</Text>
                  <Text style={s.small}>25 MIN</Text>
                </View>
                <Card>
                  <View style={s.focusHeader}>
                    <Text style={s.timer}>{duration(remaining)}</Text>
                    <View style={s.focusBadge}>
                      <Text style={s.focusBadgeText}>
                        {focus
                          ? focus.paused
                            ? 'ÎN PAUZĂ'
                            : 'ÎN FOCUS'
                          : 'POMODORO'}
                      </Text>
                    </View>
                  </View>
                  <Caption>
                    {focus?.paused
                      ? 'Poți reveni când ești pregătit.'
                      : focus
                      ? 'Respiră. Restul poate aștepta.'
                      : 'Alege un lucru. Mira ține timpul.'}
                  </Caption>
                  <View style={s.row}>
                    <View style={s.grow}>
                      <Button
                        disabled={busy}
                        title={
                          !focus
                            ? 'Începe focus'
                            : focus.paused
                            ? 'Continuă'
                            : 'Pauză'
                        }
                        onPress={() =>
                          action(
                            !focus
                              ? 'focusStart'
                              : focus.paused
                              ? 'focusResume'
                              : 'focusPause',
                          )
                        }
                      />
                    </View>
                    {!!focus && (
                      <Button
                        quiet
                        title="Oprește"
                        disabled={busy}
                        onPress={() => action('focusStop')}
                      />
                    )}
                  </View>
                </Card>
                <View style={s.sectionRow}>
                  <Text style={s.section}>Lucrurile mici contează</Text>
                </View>
                <View style={[s.row, s.statsRow]}>
                  <View style={s.grow}>
                    <Card>
                      <Text style={s.stat}>
                        {data.today.water ?? 0}
                        <Text style={s.statUnit}> pahare</Text>
                      </Text>
                      <Caption>Un moment de hidratare.</Caption>
                      <Button
                        quiet
                        disabled={busy}
                        title="+ Un pahar"
                        onPress={() => action('water')}
                      />
                    </Card>
                  </View>
                  <View style={s.grow}>
                    <Card>
                      <Text style={s.stat}>
                        {data.today.sessions ?? 0}
                        <Text style={s.statUnit}> sesiuni</Text>
                      </Text>
                      <Caption>
                        {data.today.completed ?? 0} remindere rezolvate azi
                      </Caption>
                      <Text style={s.tag}>PAS CU PAS</Text>
                    </Card>
                  </View>
                </View>
                <View style={s.sectionRow}>
                  <Text style={s.section}>Urmează</Text>
                  <Pressable onPress={() => setTab('reminders')}>
                    <Text style={s.link}>Vezi toate →</Text>
                  </Pressable>
                </View>
                <Card>
                  {upcoming.length ? (
                    <>
                      <Text style={s.cardTitle}>{upcoming[0].title}</Text>
                      <Caption>
                        {new Date(upcoming[0].due).toLocaleString('ro-RO', {
                          day: 'numeric',
                          month: 'short',
                          hour: '2-digit',
                          minute: '2-digit',
                        })}
                        {upcoming[0].daily ? ' · zilnic' : ''}
                      </Caption>
                    </>
                  ) : (
                    <>
                      <Text style={s.cardTitle}>Nimic de urmărit acum.</Text>
                      <Caption>Adaugă primul reminder când ai nevoie.</Caption>
                    </>
                  )}
                </Card>
              </>
            )}
            {tab === 'reminders' && (
              <>
                <Caption>
                  Prânz, o pauză sau un telefon important. Mira îți păstrează
                  loc pentru fiecare.
                </Caption>
                <Card>
                  <Text style={s.cardTitle}>Un reminder nou</Text>
                  <Text style={s.label}>Ce vrei să-ți amintesc?</Text>
                  <TextInput
                    accessibilityLabel="Titlu reminder"
                    value={title}
                    onChangeText={setTitle}
                    maxLength={160}
                    placeholder="De exemplu: Pauza de prânz"
                    placeholderTextColor="#718092"
                    style={s.input}
                  />
                  <View style={s.row}>
                    <View style={s.grow}>
                      <Text style={s.label}>Peste minute</Text>
                      <TextInput
                        accessibilityLabel="Peste minute"
                        keyboardType="number-pad"
                        value={minutes}
                        onChangeText={v => {
                          setMinutes(v);
                          setClock('');
                        }}
                        style={s.input}
                        maxLength={5}
                      />
                    </View>
                    <View style={s.grow}>
                      <Text style={s.label}>Sau la ora</Text>
                      <TextInput
                        accessibilityLabel="Ora reminderului"
                        value={clock}
                        onChangeText={setClock}
                        placeholder="13:00"
                        placeholderTextColor="#718092"
                        style={s.input}
                        maxLength={5}
                      />
                    </View>
                  </View>
                  <View style={s.settingRow}>
                    <Text style={s.body}>Repetă zilnic</Text>
                    <Switch
                      accessibilityLabel="Repetă zilnic"
                      value={daily}
                      onValueChange={setDaily}
                      trackColor={{ true: '#32644f', false: '#35404d' }}
                      thumbColor={daily ? '#85efb7' : '#a1aab6'}
                    />
                  </View>
                  <Caption>
                    {clock
                      ? 'Dacă ora a trecut, prima notificare va fi mâine.'
                      : 'Poți alege o durată sau o oră fixă.'}
                  </Caption>
                  <Button
                    disabled={busy}
                    title="Adaugă reminder"
                    onPress={addReminder}
                  />
                </Card>
                <View style={s.sectionRow}>
                  <Text style={s.section}>Pe lista ta</Text>
                  <Text style={s.small}>{upcoming.length} ACTIVE</Text>
                </View>
                {!upcoming.length && (
                  <Card>
                    <Text style={s.cardTitle}>Deocamdată, liniște.</Text>
                    <Caption>Reminderele tale vor apărea aici.</Caption>
                  </Card>
                )}
                {upcoming.map(r => (
                  <Card key={r.id}>
                    <View style={s.sectionRow}>
                      <Text style={[s.cardTitle, s.grow]}>{r.title}</Text>
                      <Pressable
                        accessibilityLabel={`Șterge ${r.title}`}
                        onPress={() => deleteReminder(r)}
                        style={s.delete}
                      >
                        <Text style={s.muted}>×</Text>
                      </Pressable>
                    </View>
                    <Caption>
                      {new Date(r.due).toLocaleString('ro-RO')}
                      {r.daily ? ' · se repetă zilnic' : ''}
                    </Caption>
                    <Text
                      style={[
                        s.countdown,
                        r.due - now <= 60000 && s.urgentText,
                      ]}
                    >
                      {r.due <= now
                        ? 'E timpul'
                        : `În ${duration(r.due - now)}`}
                    </Text>
                    <View style={s.row}>
                      <View style={s.grow}>
                        <Button
                          title="Gata ✓"
                          disabled={busy}
                          onPress={() =>
                            action('complete', { id: r.id, due: r.due })
                          }
                        />
                      </View>
                      <Button
                        quiet
                        title="+5 min"
                        disabled={busy}
                        onPress={() =>
                          action('snooze', { id: r.id, due: r.due })
                        }
                      />
                    </View>
                  </Card>
                ))}
              </>
            )}
            {tab === 'studio' && (
              <>
                <View style={s.hero}>
                  <Eyes
                    theme={data.theme}
                    expression={expression}
                    motion={data.motion !== false}
                  />
                  <Text style={s.heroTitle}>{theme.name}</Text>
                  <Text style={s.heroText}>
                    {theme.detail}. Aceeași Mira, altă dispoziție.
                  </Text>
                </View>
                <Text style={s.section}>Alege un companion</Text>
                <View style={s.themeGrid}>
                  {themes.map(t => (
                    <Pressable
                      key={t.id}
                      accessibilityRole="radio"
                      accessibilityState={{ checked: data.theme === t.id }}
                      onPress={() => action('settings', { theme: t.id })}
                      style={[
                        s.themeCard,
                        data.theme === t.id && { borderColor: t.color },
                      ]}
                    >
                      <Text style={[s.themeEyes, { color: t.color }]}>
                        {t.id === 'pixel'
                          ? '▪ ▪'
                          : t.id === 'cat'
                          ? '◖ ◗'
                          : '● ●'}
                      </Text>
                      <Text style={s.cardTitle}>{t.name}</Text>
                      <Text style={s.small}>{t.detail}</Text>
                    </Pressable>
                  ))}
                </View>
                <Text style={s.section}>O mică repetiție</Text>
                <Caption>
                  Expresia apare aici și în capsulă, dacă este pornită.
                </Caption>
                <View style={s.wrap}>
                  {[
                    { id: 'happy', name: 'Curioasă' },
                    { id: 'wink', name: 'Cu un clinchet' },
                    { id: 'sleepy', name: 'Somnoroasă' },
                    { id: 'excited', name: 'Entuziasmată' },
                  ].map(e => (
                    <Button
                      key={e.id}
                      quiet
                      title={e.name}
                      onPress={() => {
                        setExpression(e.id);
                        if (data.overlayRunning)
                          run(() => companion.preview(e.id));
                      }}
                    />
                  ))}
                </View>
              </>
            )}
            {tab === 'voice' && <VoicePanel data={data} run={run} />}
            {tab === 'settings' && (
              <>
                <Card>
                  <View style={s.sectionRow}>
                    <Text style={s.cardTitle}>Mira peste alte aplicații</Text>
                    <View
                      style={[
                        s.dot,
                        data.overlayRunning ? s.runningDot : s.stoppedDot,
                      ]}
                    />
                  </View>
                  <Caption>
                    O capsulă mică, mereu la îndemână. Nu acoperă comenzile
                    sistemului. Se oprește din buton sau din notificare.
                  </Caption>
                  <Button
                    disabled={busy}
                    title={
                      !data.overlayGranted
                        ? 'Permite afișarea capsulei'
                        : data.overlayRunning
                        ? 'Oprește capsula'
                        : 'Pornește capsula'
                    }
                    onPress={toggleOverlay}
                  />
                  <Caption>
                    {data.overlayGranted
                      ? 'Permisiunea de afișare este acordată.'
                      : 'Android va deschide pagina de permisiuni. Revino apoi pentru a o porni.'}
                  </Caption>
                </Card>
                <Card>
                  <Text style={s.cardTitle}>Notificări pentru remindere</Text>
                  <Caption>
                    {data.notificationsGranted
                      ? 'Sunt activate. Te anunțăm și când aplicația nu este deschisă.'
                      : 'Activează notificările ca să primești remindere cu aplicația închisă.'}
                  </Caption>
                  {!data.notificationsGranted && (
                    <Button
                      quiet
                      title="Permite notificările"
                      onPress={notifyPermission}
                    />
                  )}
                  <Caption>
                    În repaus profund, Android poate întârzia notificările.
                    Capsula nu pornește singură după repornirea telefonului.
                  </Caption>
                </Card>
                <Card>
                  <Text style={s.cardTitle}>Cum se așază capsula</Text>
                  <Text style={s.label}>Poziționare la cameră</Text>
                  <View style={s.row}>
                    <Button
                      title="Notch cu aripi · auto"
                      quiet={data.cameraMode === false}
                      onPress={() => action('settings', { cameraMode: true })}
                    />
                    <Button
                      title="Capsulă"
                      quiet={data.cameraMode !== false}
                      onPress={() => action('settings', { cameraMode: false })}
                    />
                  </View>
                  <Caption>
                    Camera se unește cu forma neagră de sus, cu doi ochi
                    dedesubt și lumină cyan, dacă Android raportează decupajul.
                    Nu poate desena peste gaura fizică sau peste elementele
                    protejate ale sistemului.
                  </Caption>
                  <View style={s.row}>
                    <Button
                      quiet
                      title="←"
                      onPress={() =>
                        action('settings', { cameraX: (data.cameraX ?? 0) - 2 })
                      }
                    />
                    <Button
                      quiet
                      title="→"
                      onPress={() =>
                        action('settings', { cameraX: (data.cameraX ?? 0) + 2 })
                      }
                    />
                    <Button
                      quiet
                      title="↑"
                      onPress={() =>
                        action('settings', { cameraY: (data.cameraY ?? 0) - 2 })
                      }
                    />
                    <Button
                      quiet
                      title="↓"
                      onPress={() =>
                        action('settings', { cameraY: (data.cameraY ?? 0) + 2 })
                      }
                    />
                  </View>
                  <Text style={s.label}>Dimensiune</Text>
                  <View style={s.wrap}>
                    {[0.85, 1, 1.15].map(scale => (
                      <Button
                        key={scale}
                        quiet={data.scale !== scale}
                        title={`${Math.round(scale * 100)}%`}
                        onPress={() => action('settings', { scale })}
                      />
                    ))}
                  </View>
                  <Text style={s.label}>Distanță față de bara de sus</Text>
                  <View style={s.row}>
                    <Button
                      quiet
                      title="−"
                      onPress={() =>
                        action('settings', { offset: data.offset - 8 })
                      }
                    />
                    <Text style={s.body}>{data.offset} dp</Text>
                    <Button
                      quiet
                      title="+"
                      onPress={() =>
                        action('settings', { offset: data.offset + 8 })
                      }
                    />
                  </View>
                  <View style={s.settingRow}>
                    <Text style={s.body}>Animații</Text>
                    <Switch
                      accessibilityLabel="Animații"
                      value={data.motion !== false}
                      onValueChange={motion => {
                        action('settings', { motion });
                      }}
                      trackColor={{ true: '#32644f', false: '#35404d' }}
                      thumbColor="#85efb7"
                    />
                  </View>
                </Card>
                <Card>
                  <Text style={s.cardTitle}>
                    Telefonul și desktopul, împreună.
                  </Text>
                  <Caption>
                    Asociază PC-ul din Voce &amp; PC. Transcrierile finalizate
                    se trimit criptat pe aceeași rețea; tastele sunt definite pe
                    desktop.
                  </Caption>
                  <Button
                    title="Deschide Voce & PC"
                    onPress={() => setTab('voice')}
                  />
                </Card>
                <Text style={s.footer}>
                  Mira pentru Android · {data.appVersion ?? 'preview'}
                  {'\n'}Fără cont. Fără telemetrie. În ritmul tău.
                </Text>
              </>
            )}
          </>
        )}
      </ScrollView>
      <View style={s.nav} accessibilityRole="tablist">
        {tabs.map(t => (
          <Pressable
            key={t.id}
            accessibilityRole="tab"
            accessibilityLabel={t.title}
            accessibilityState={{ selected: tab === t.id }}
            onPress={() => {
              setError('');
              setTab(t.id);
            }}
            style={({ pressed }) => [s.navItem, pressed && { opacity: 0.65 }]}
          >
            <View style={[s.navIconBox, tab === t.id && s.navIconSelected]}>
              <NavIcon name={t.id} active={tab === t.id} />
            </View>
            <Text
              numberOfLines={1}
              maxFontSizeMultiplier={1.3}
              style={[s.navText, tab === t.id && s.active]}
            >
              {t.title}
            </Text>
          </Pressable>
        ))}
      </View>
    </SafeAreaView>
  );
}
export default function App() {
  return (
    <SafeAreaProvider>
      <MiraApp />
    </SafeAreaProvider>
  );
}
const s = StyleSheet.create({
  root: { flex: 1, backgroundColor: '#0b1016' },
  content: { paddingHorizontal: 20, paddingTop: 8, paddingBottom: 24, gap: 14 },
  brandRow: {
    paddingHorizontal: 24,
    paddingVertical: 12,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  brand: {
    fontSize: 24,
    color: '#eef5f4',
    fontWeight: '700',
    letterSpacing: -1,
  },
  brandDot: {
    width: 9,
    height: 9,
    backgroundColor: '#85efb7',
    borderRadius: 5,
  },
  local: { fontSize: 9, color: '#8d9fae', letterSpacing: 1.2 },
  row: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  grow: { flex: 1 },
  statsRow: { alignItems: 'flex-start' },
  eyebrow: { color: '#91a7b7', fontSize: 12, letterSpacing: 0.5 },
  heading: {
    fontSize: 27,
    lineHeight: 33,
    fontWeight: '700',
    color: '#eef5f4',
    letterSpacing: -1.3,
    marginBottom: 5,
  },
  hero: {
    backgroundColor: '#10272c',
    borderColor: '#243f45',
    borderWidth: 1,
    padding: 20,
    paddingTop: 15,
    borderRadius: 24,
    marginBottom: 8,
  },
  urgentHero: { backgroundColor: '#281b25', borderColor: '#6b3746' },
  heroTop: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  chip: { fontSize: 9, color: '#9db6bd', letterSpacing: 1.5 },
  urgentDot: { backgroundColor: '#ff899e' },
  runningDot: { backgroundColor: '#85efb7' },
  stoppedDot: { backgroundColor: '#718092' },
  dot: { width: 7, height: 7, borderRadius: 4 },
  heroTitle: {
    fontSize: 17,
    color: '#eef5f4',
    fontWeight: '600',
    textAlign: 'center',
    marginBottom: 8,
  },
  heroText: {
    fontSize: 12,
    color: '#9eb1bb',
    textAlign: 'center',
    lineHeight: 19,
  },
  sectionRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    gap: 8,
  },
  section: {
    color: '#e3ecef',
    fontWeight: '600',
    fontSize: 16,
    marginTop: 2,
    flexShrink: 1,
  },
  small: { fontSize: 10, color: '#8da1b3', lineHeight: 17 },
  card: {
    backgroundColor: '#141c26',
    borderRadius: 23,
    padding: 18,
    gap: 12,
    borderColor: '#222e3b',
    borderWidth: 1,
  },
  cardTitle: {
    color: '#e9f0f3',
    fontWeight: '600',
    fontSize: 16,
    lineHeight: 23,
  },
  caption: { color: '#9bafbd', fontSize: 12, lineHeight: 20 },
  focusHeader: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 8,
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  focusBadge: {
    backgroundColor: '#263a35',
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 12,
  },
  focusBadgeText: {
    color: '#93f4c5',
    fontSize: 10,
    fontWeight: '700',
    letterSpacing: 1,
  },
  timer: {
    color: '#eff9f3',
    fontSize: 46,
    letterSpacing: -2,
    fontWeight: '300',
    fontVariant: ['tabular-nums'],
  },
  stat: { color: '#eef5f4', fontSize: 30, fontWeight: '600' },
  statUnit: { fontSize: 11, color: '#9bafbd', fontWeight: '400' },
  tag: { fontSize: 9, color: '#85efb7', letterSpacing: 1.3, lineHeight: 20 },
  button: {
    minHeight: 50,
    paddingHorizontal: 18,
    paddingVertical: 13,
    borderRadius: 15,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: '#85efb7',
  },
  buttonText: { fontSize: 14, color: '#10251c', fontWeight: '700' },
  quiet: { backgroundColor: '#202c39' },
  quietText: { color: '#d0dce6' },
  link: { fontSize: 11, color: '#85efb7' },
  label: { fontSize: 11, color: '#a3b7c7', marginTop: 2 },
  input: {
    backgroundColor: '#0e151e',
    borderRadius: 13,
    borderWidth: 1,
    borderColor: '#2b3a4b',
    padding: 13,
    color: '#eef5f4',
    fontSize: 14,
  },
  settingRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  body: { color: '#d9e5eb', fontSize: 13 },
  countdown: { color: '#85efb7', fontSize: 16, fontVariant: ['tabular-nums'] },
  urgentText: { color: '#ff899e' },
  delete: {
    minWidth: 44,
    minHeight: 44,
    justifyContent: 'center',
    alignItems: 'center',
  },
  muted: { color: '#a5b3c0', fontSize: 24 },
  error: {
    backgroundColor: '#38202b',
    color: '#ffc0ce',
    padding: 15,
    borderRadius: 14,
    lineHeight: 20,
  },
  themeGrid: { flexDirection: 'row', flexWrap: 'wrap', gap: 12 },
  themeCard: {
    width: '48%',
    flexGrow: 1,
    backgroundColor: '#141c26',
    borderWidth: 1,
    borderColor: '#273342',
    borderRadius: 22,
    padding: 18,
    gap: 8,
  },
  themeEyes: { fontSize: 30, letterSpacing: 8, marginBottom: 10 },
  wrap: { flexDirection: 'row', flexWrap: 'wrap', gap: 10 },
  footer: {
    textAlign: 'center',
    color: '#8092a2',
    fontSize: 11,
    lineHeight: 21,
    paddingVertical: 10,
  },
  nav: {
    borderTopWidth: 1,
    borderColor: '#1b2931',
    flexDirection: 'row',
    marginHorizontal: 18,
    marginBottom: 4,
    paddingVertical: 5,
    paddingHorizontal: 6,
    borderRadius: 22,
    backgroundColor: '#101d25',
  },
  navItem: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 3,
    minHeight: 54,
  },
  navIconBox: {
    width: 42,
    height: 30,
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 15,
    overflow: 'hidden',
  },
  navIconSelected: { backgroundColor: '#1b302c' },
  navText: { color: '#92a4b1', fontSize: 10, fontWeight: '500' },
  active: { color: '#93f4c5', fontWeight: '600' },
});
