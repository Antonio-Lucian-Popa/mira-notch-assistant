import React, { useState } from 'react';
import {
  Alert,
  PermissionsAndroid,
  Platform,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { companion } from './native';
import type { Snapshot, Transcript, RemoteAction } from './model';
type Props = {
  data: Snapshot;
  run: (fn: () => Promise<void>) => Promise<boolean>;
};
function Button({
  title,
  onPress,
  danger = false,
}: {
  title: string;
  onPress: () => void;
  danger?: boolean;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={title}
      onPress={onPress}
      style={[s.button, danger && s.stop]}
    >
      <Text style={s.buttonText}>{title}</Text>
    </Pressable>
  );
}
export function VoicePanel({ data, run }: Props) {
  const [code, setCode] = useState('');
  const [actions, setActions] = useState<RemoteAction[]>([]);
  const [opened, setOpened] = useState<Transcript>();
  const voice = data.voice;
  const peer = data.peer;
  const start = (dictate: boolean) =>
    run(async () => {
      const granted = await PermissionsAndroid.request(
        PermissionsAndroid.PERMISSIONS.RECORD_AUDIO,
      );
      if (granted !== PermissionsAndroid.RESULTS.GRANTED)
        throw Error(
          'Microfonul nu a fost permis. Îl poți activa din setările Android.',
        );
      if (Number(Platform.Version) >= 33)
        await PermissionsAndroid.request(
          PermissionsAndroid.PERMISSIONS.POST_NOTIFICATIONS,
        );
      await companion.voiceStart(dictate);
    });
  return (
    <>
      <View style={s.card}>
        <Text style={s.title}>Mira, te ascult.</Text>
        <Text style={s.body}>
          Comenzi fixe, fără chatbot. Serviciul vocal Android poate procesa
          vocea local sau online, în funcție de telefon și limbă.
        </Text>
        <Text accessibilityLiveRegion="polite" style={s.status}>
          {voice?.message ?? 'Microfon oprit.'}
        </Text>
        {voice?.active ? (
          <Button
            danger
            title="Stop microfon"
            onPress={() => run(() => companion.voiceStop())}
          />
        ) : (
          <>
            <Button
              title="Ascultă «Mira» · 60 secunde"
              onPress={() => start(false)}
            />
            <Button title="Începe transcrierea" onPress={() => start(true)} />
          </>
        )}
        {!!voice?.partial && (
          <Text selectable style={s.partial}>
            {voice.partial.slice(-1800)}
          </Text>
        )}
        <Text style={s.small}>
          «Mira, pornește transcrierea» · «Mira gata» oprește fără a include
          comanda în text. Mai poți cere focus, pauză, continuare sau un pahar
          de apă. Ascultarea permanentă în fundal nu este activată.
        </Text>
      </View>
      <View style={s.card}>
        <Text style={s.title}>Transcrierile tale</Text>
        <Text style={s.small}>
          Sunt păstrate pe telefon. Verifică textul recunoscut. Exportul PDF
          păstrează textul, fără rezumat generat.
        </Text>
        {!data.transcripts?.length && (
          <Text style={s.body}>Prima conversație începe cu un cuvânt.</Text>
        )}
        {(data.transcripts ?? []).map(row => (
          <View key={row.id} style={s.record}>
            <Text style={s.recordTitle}>
              {new Date(row.createdAt).toLocaleString('ro-RO')}
            </Text>
            <Text style={s.small}>
              {row.complete
                ? row.synced
                  ? 'Primit pe PC'
                  : 'Salvat · în așteptare'
                : 'Schiță / sesiune în curs'}
            </Text>
            <Text numberOfLines={3} style={s.body}>
              {row.preview || 'Încă nu a fost confirmat text.'}
            </Text>
            <View style={s.wrap}>
              <Button
                title="Citește"
                onPress={() =>
                  run(async () => {
                    setOpened(await companion.transcript(row.id));
                  })
                }
              />
              {row.complete && (
                <Button
                  title="Exportă PDF"
                  onPress={() => run(() => companion.sharePdf(row.id))}
                />
              )}
              <Button
                title="Șterge"
                danger
                onPress={() =>
                  Alert.alert(
                    'Ștergi de pe telefon?',
                    'Fișierul primit deja pe PC rămâne acolo.',
                    [
                      { text: 'Păstrează', style: 'cancel' },
                      {
                        text: 'Șterge',
                        style: 'destructive',
                        onPress: () =>
                          run(() => companion.deleteTranscript(row.id)),
                      },
                    ],
                  )
                }
              />
            </View>
          </View>
        ))}
        {!!opened && (
          <View style={s.record}>
            <Text selectable style={s.partial}>
              {opened.text}
            </Text>
            <Button
              title="Închide textul"
              onPress={() => setOpened(undefined)}
            />
          </View>
        )}
      </View>
      <View style={s.card}>
        <Text style={s.title}>PC-ul tău, aproape.</Text>
        <Text style={s.body}>
          Pe desktop, deschide Telefon și pornește primirea. Asocierea
          autorizează transferul transcrierilor și shortcuturile definite de
          tine pe PC.
        </Text>
        <Text style={s.status}>{peer?.message ?? 'Niciun PC asociat.'}</Text>
        {!peer?.paired ? (
          <>
            <TextInput
              accessibilityLabel="Cod asociere PC"
              value={code}
              onChangeText={setCode}
              placeholder="mira-pair://…"
              placeholderTextColor="#718092"
              autoCapitalize="none"
              autoCorrect={false}
              style={s.input}
            />
            <View style={s.wrap}>
              <Button
                title="Fotografiază QR-ul"
                onPress={() =>
                  run(async () => {
                    setCode(await companion.scanPairing());
                  })
                }
              />
              <Button
                title="Asociază PC-ul"
                onPress={() => run(() => companion.pair(code))}
              />
            </View>
          </>
        ) : (
          <>
            <View style={s.wrap}>
              <Button
                title="Sincronizează acum"
                onPress={() => run(() => companion.syncNow())}
              />
              <Button
                title="Încarcă telecomanda"
                onPress={() =>
                  run(async () => {
                    setActions(await companion.remoteActions());
                  })
                }
              />
              <Button
                danger
                title="Deconectează PC-ul"
                onPress={() =>
                  run(async () => {
                    await companion.unpair();
                    setActions([]);
                  })
                }
              />
            </View>
            {actions.map(action => (
              <Button
                key={action.id}
                title={`${action.name}${
                  action.keys.length ? ' · ' + action.keys.join('+') : ''
                }`}
                onPress={() =>
                  run(async () => {
                    await companion.remoteAction(action.id);
                    Alert.alert(
                      'Trimis către PC',
                      'Acțiunea a fost acceptată în coadă. PC-ul trebuie să fie activ; combinațiile rezervate de sistem pot fi ignorate.',
                    );
                  })
                }
              />
            ))}
          </>
        )}
        <Text style={s.small}>
          Aceeași rețea locală, PC pornit și primire activă. Android programează
          transferul automat pe rețele fără tarifare; poate întârzia.
          Proximitatea fizică singură nu declanșează transferul. Comenzile de
          taste nu sunt păstrate pentru executare ulterioară când PC-ul este
          offline.
        </Text>
      </View>
    </>
  );
}
const s = StyleSheet.create({
  card: {
    padding: 18,
    borderRadius: 23,
    backgroundColor: '#141c26',
    borderWidth: 1,
    borderColor: '#283a49',
    gap: 13,
  },
  title: { color: '#edf5f4', fontSize: 20, fontWeight: '600' },
  body: { color: '#b3c7d4', fontSize: 13, lineHeight: 21 },
  small: { color: '#8fa6b9', fontSize: 11, lineHeight: 19 },
  status: { color: '#85efb7', fontSize: 13, lineHeight: 21 },
  button: {
    backgroundColor: '#23394a',
    paddingHorizontal: 15,
    paddingVertical: 13,
    borderRadius: 13,
    minHeight: 44,
  },
  buttonText: { color: '#d3f1e4', fontSize: 12, fontWeight: '600' },
  stop: { backgroundColor: '#54303d' },
  input: {
    color: '#eff5f7',
    backgroundColor: '#0e151e',
    borderRadius: 12,
    padding: 13,
    borderWidth: 1,
    borderColor: '#3c5368',
  },
  wrap: { flexDirection: 'row', flexWrap: 'wrap', gap: 9 },
  record: {
    paddingTop: 12,
    borderTopWidth: 1,
    borderColor: '#2a3c4a',
    gap: 10,
  },
  recordTitle: { color: '#edf5f4', fontSize: 14, fontWeight: '500' },
  partial: {
    color: '#eff5f5',
    fontSize: 13,
    lineHeight: 21,
    fontFamily: 'monospace',
  },
});
