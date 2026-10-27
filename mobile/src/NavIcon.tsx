import React from 'react';
import { View, StyleSheet } from 'react-native';

type Name = 'today' | 'reminders' | 'studio' | 'voice' | 'settings';
/** Fixed-size line icons avoid font-dependent Unicode glyph sizing. */
export function NavIcon({ name, active }: { name: Name; active: boolean }) {
  const color = active ? '#93f4c5' : '#8c9fab';
  const line = { borderColor: color };
  return (
    <View
      style={s.canvas}
      accessible={false}
      importantForAccessibility="no-hide-descendants"
    >
      {name === 'today' && (
        <>
          <View style={[s.roof, line]} />
          <View style={[s.house, line]}>
            <View style={[s.door, line]} />
          </View>
        </>
      )}
      {name === 'reminders' && (
        <>
          <View style={[s.clock, line]} />
          <View style={[s.hand, { backgroundColor: color }]} />
          <View style={[s.minute, { backgroundColor: color }]} />
        </>
      )}
      {name === 'studio' && (
        <>
          <View style={[s.face, line]} />
          {[9, 18].map(left => (
            <View
              key={left}
              style={[s.eye, { left, backgroundColor: color }]}
            />
          ))}
        </>
      )}
      {name === 'voice' && (
        <>
          <View style={[s.mic, line]} />
          <View style={[s.micCup, line]} />
          <View style={[s.stem, { backgroundColor: color }]} />
        </>
      )}
      {name === 'settings' && (
        <>
          {[7, 15, 23].map((top, i) => (
            <React.Fragment key={top}>
              <View style={[s.slider, { top, backgroundColor: color }]} />
              <View
                style={[
                  s.knob,
                  line,
                  { top: top - 3 },
                  i === 1 ? s.knobRight : s.knobLeft,
                ]}
              />
            </React.Fragment>
          ))}
        </>
      )}
    </View>
  );
}
const s = StyleSheet.create({
  canvas: { width: 32, height: 32, transform: [{ scale: 0.6875 }] },
  roof: {
    position: 'absolute',
    width: 17,
    height: 17,
    top: 5,
    left: 7.5,
    borderLeftWidth: 2.3,
    borderTopWidth: 2.3,
    transform: [{ rotate: '45deg' }],
    borderRadius: 2,
  },
  house: {
    position: 'absolute',
    left: 6,
    top: 13,
    width: 20,
    height: 16,
    borderWidth: 2.3,
    borderTopWidth: 0,
    borderRadius: 3,
    alignItems: 'center',
    justifyContent: 'flex-end',
  },
  door: {
    width: 6,
    height: 10,
    borderWidth: 2,
    borderBottomWidth: 0,
    borderTopLeftRadius: 2,
    borderTopRightRadius: 2,
  },
  clock: {
    position: 'absolute',
    left: 3,
    top: 3,
    width: 26,
    height: 26,
    borderWidth: 2.3,
    borderRadius: 14,
  },
  hand: {
    position: 'absolute',
    left: 15,
    top: 8,
    width: 2.3,
    height: 9,
    borderRadius: 2,
  },
  minute: {
    position: 'absolute',
    left: 15,
    top: 15,
    width: 8,
    height: 2.3,
    borderRadius: 2,
  },
  face: {
    position: 'absolute',
    left: 2,
    top: 6,
    width: 28,
    height: 22,
    borderWidth: 2.3,
    borderRadius: 9,
  },
  eye: { position: 'absolute', top: 13, width: 4, height: 8, borderRadius: 3 },
  mic: {
    position: 'absolute',
    left: 11,
    top: 2,
    width: 10,
    height: 18,
    borderWidth: 2.3,
    borderRadius: 7,
  },
  micCup: {
    position: 'absolute',
    left: 6,
    top: 13,
    width: 20,
    height: 13,
    borderWidth: 2.3,
    borderTopWidth: 0,
    borderBottomLeftRadius: 12,
    borderBottomRightRadius: 12,
  },
  stem: {
    position: 'absolute',
    left: 15,
    top: 25,
    width: 2.3,
    height: 6,
    borderRadius: 2,
  },
  slider: {
    position: 'absolute',
    left: 3,
    width: 26,
    height: 2,
    borderRadius: 2,
  },
  knobRight: { left: 18 },
  knobLeft: { left: 9 },
  knob: {
    position: 'absolute',
    width: 8,
    height: 8,
    borderRadius: 4,
    borderWidth: 2,
    backgroundColor: '#15252b',
  },
});
