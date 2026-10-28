import React, { useState } from 'react';
import {
  Alert,
  PermissionsAndroid,
  Platform,
  Pressable,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  View,
} from 'react-native';
import { companion } from './native';
import { Eyes } from './Eyes';
import type { Snapshot, Transcript, RemoteAction } from './model';
type Props = {
  data: Snapshot;
  run: (fn: () => Promise<void>) => Promise<boolean>;
};
function Button({
  title,
  onPress,
  danger = false,
  disabled = false,
}: {
  title: string;
  onPress: () => void;
  danger?: boolean;
  disabled?: boolean;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={title}
      onPress={onPress}
      disabled={disabled}
      accessibilityState={{ disabled }}
      style={[s.button, danger && s.stop, disabled && s.disabled]}
    >
      <Text style={s.buttonText}>{title}</Text>
    </Pressable>
  );
}
export function VoicePanel({ data, run }: Props) {
  const [code, setCode] = useState('');
  const [details, setDetails] = useState(false);
  const [actions, setActions] = useState<RemoteAction[]>([]);
  const [opened, setOpened] = useState<Transcript>();
  const voice = data.voice;
  const model = data.voiceModel;
  const peer = data.peer;
  const start = (dictate: boolean, automatic = false) =>
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
      if (automatic) await companion.voiceAuto(true);
      else await companion.voiceStart(dictate);
    });
  return (
    <>
      {!model?.ready && (
        <View style={s.card}>
          <Text style={s.title}>Vocea Mira, pe telefon.</Text>
          <Text style={s.body}>
            Un model gratuit de aproximativ 148 MB, descărcat o singură dată.
            Recunoaște româna local, fără cont, abonament sau trimiterea
            audio-ului la un server.
          </Text>
          <Text accessibilityLiveRegion="polite" style={s.status}>
            {model?.message ?? 'Descarcă modelul pentru a activa vocea.'}
            {model?.downloading ? ` · ${model.progress}%` : ''}
          </Text>
          {model?.downloading && (
            <View style={s.progressTrack}>
              <View style={[s.progressFill, { width: `${model.progress}%` }]} />
            </View>
          )}
          <Button
            title={
              model?.downloading
                ? 'Se descarcă…'
                : 'Descarcă modelul gratuit · 148 MB'
            }
            disabled={model?.downloading === true}
            onPress={() => run(() => companion.voiceModelDownload())}
          />
        </View>
      )}
      <View style={[s.card, s.voiceHero]}>
        <View style={s.switchRow}>
          <View style={s.grow}>
            <Text style={s.overline}>ASISTENT VOCAL</Text>
            <Text style={s.title}>
              {voice?.active
                ? 'Sunt aici. Te ascult.'
                : 'Totul începe cu „Mira”.'}
            </Text>
          </View>
          <Switch
            accessibilityLabel="Activare vocală Mira"
            value={data.voiceEnabled === true}
            disabled={!model?.ready}
            trackColor={{ false: '#344650', true: '#356750' }}
            thumbColor={data.voiceEnabled ? '#93f4c5' : '#b3c5ce'}
            onValueChange={enabled => {
              if (enabled) start(false, true);
              else run(() => companion.voiceAuto(false));
            }}
          />
        </View>
        <Eyes
          compact
          theme={data.theme}
          motion={data.motion !== false}
          expression={voice?.active ? 'excited' : 'happy'}
        />
        <Text
          accessibilityLiveRegion="polite"
          style={[s.status, voice?.state === 'error' && s.errorStatus]}
        >
          {voice?.message ?? 'Activează o dată, apoi spune „Mira”.'}
        </Text>
        <Text style={s.body}>
          Spune „Mira”, apoi comanda. Fără să apeși microfonul de fiecare dată.
        </Text>
        <View style={s.commandList}>
          {['„Pornește focus”', '„Am băut apă”', '„Pornește transcrierea”'].map(
            text => (
              <View key={text} style={s.command}>
                <Text style={s.commandText}>{text}</Text>
              </View>
            ),
          )}
        </View>
        {voice?.state === 'dictating' ? (
          <Button
            title="Oprește microfonul și salvează"
            danger
            onPress={() => run(() => companion.voiceStop())}
          />
        ) : (
          <Button
            title="Transcriere nouă"
            disabled={!model?.ready}
            onPress={() => start(true)}
          />
        )}
        {!!data.voiceEnabled && !voice?.active && (
          <Button
            title="Reia activarea vocală"
            onPress={() => start(false, true)}
          />
        )}
        {!!voice?.partial && (
          <Text selectable style={s.partial}>
            {voice.partial.slice(-1800)}
          </Text>
        )}
        <Pressable
          accessibilityRole="button"
          accessibilityState={{ expanded: details }}
          onPress={() => setDetails(!details)}
          style={s.detailsButton}
        >
          <Text style={s.detailsText}>
            {details ? 'Ascunde detaliile −' : 'Cum funcționează +'}{' '}
          </Text>
        </Pressable>
        {details && (
          <Text style={s.small}>
            Whisper rulează pe telefon și recunoaște româna fără pachetele
            Google sau Samsung. Activează o dată, apoi spune „Mira” și comanda.
            Lasă o scurtă pauză după propoziții pentru procesare. Consumă
            baterie cât timp microfonul este activ; Android poate întrerupe
            serviciul.{'\n\n'}
            „Mira gata” salvează transcrierea și revine la ascultare. Oprește
            complet din comutator sau notificare. Nu păstrăm înregistrări audio;
            comenzile sunt fixe, fără chatbot. Licențe: Whisper și whisper.cpp,
            MIT.
          </Text>
        )}
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
          Scanează codul din pagina Telefon de pe desktop. Transcrierile și
          comenzile tale, conectate în siguranță.
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
                title="Scanează QR"
                onPress={() =>
                  run(async () => {
                    setCode(await companion.scanPairing());
                  })
                }
              />
              <Button
                title="Conectează"
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
            <Text style={s.recordTitle}>Merg la baie · scutură telefonul</Text>
            <Text style={s.small}>
              Trei scuturări ferme alternează Plecare / Revenire, conform listei
              salvate pe PC. Funcționează cu capsula pornită și ecranul
              telefonului aprins, inclusiv peste alte aplicații. Poți începe
              imediat următoarea secvență de trei scuturări. PC-ul trebuie să
              fie online și primirea activă. Revenirea trimite shortcutul ales;
              deblocarea securizată cere autentificarea sistemului.
            </Text>
            <Button
              title={
                data.shakeEnabled
                  ? 'Dezactivează scuturarea'
                  : 'Activează scuturarea Plecare / Revenire'
              }
              onPress={() => run(() => companion.shake(!data.shakeEnabled))}
            />
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
          Telefonul și PC-ul trebuie să fie în aceeași rețea, cu primirea
          activată pe desktop. Transferul automat poate întârzia; comenzile nu
          se execută când PC-ul este offline.
        </Text>
      </View>
    </>
  );
}
const s = StyleSheet.create({
  disabled: { opacity: 0.45 },
  progressTrack: {
    height: 5,
    backgroundColor: '#24363c',
    borderRadius: 3,
    overflow: 'hidden',
  },
  progressFill: { height: 5, backgroundColor: '#93f4c5', borderRadius: 3 },
  voiceHero: { backgroundColor: '#10272c', borderColor: '#2b4a4e' },
  switchRow: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  grow: { flex: 1 },
  overline: {
    color: '#8fc4bd',
    fontSize: 10,
    fontWeight: '700',
    letterSpacing: 1.4,
    marginBottom: 8,
  },
  commandList: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 8,
    marginBottom: 4,
  },
  commandText: { color: '#c0dedb', fontSize: 12, lineHeight: 18 },
  command: {
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 12,
    backgroundColor: '#1b383d',
  },
  detailsButton: { minHeight: 44, justifyContent: 'center' },
  detailsText: { color: '#93f4c5', fontSize: 13, fontWeight: '600' },
  card: {
    padding: 20,
    borderRadius: 26,
    backgroundColor: '#131f29',
    borderWidth: 1,
    borderColor: '#283a49',
    gap: 13,
  },
  title: { color: '#edf5f4', fontSize: 20, fontWeight: '600' },
  body: { color: '#b3c7d4', fontSize: 14, lineHeight: 22 },
  small: { color: '#9bafbd', fontSize: 12, lineHeight: 20 },
  status: { color: '#85efb7', fontSize: 13, lineHeight: 21 },
  errorStatus: { color: '#ffbd99' },
  button: {
    backgroundColor: '#23394a',
    paddingHorizontal: 15,
    paddingVertical: 13,
    borderRadius: 13,
    minHeight: 50,
    justifyContent: 'center',
    alignItems: 'center',
  },
  buttonText: { color: '#d3f1e4', fontSize: 14, fontWeight: '600' },
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
