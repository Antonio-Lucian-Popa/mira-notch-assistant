import React, { useEffect, useRef } from 'react';
import { Animated, Easing, StyleSheet, View, PanResponder } from 'react-native';
import { themes, Theme } from './model';

export function Eyes({
  theme,
  urgent = false,
  motion = true,
  expression = 'happy',
  compact = false,
}: {
  theme: Theme;
  urgent?: boolean;
  motion?: boolean;
  expression?: string;
  compact?: boolean;
}) {
  const blink = useRef(new Animated.Value(1)).current;
  const jitter = useRef(new Animated.Value(0)).current;
  const look = useRef(new Animated.ValueXY()).current;
  const color = urgent ? '#ff899e' : themes.find(t => t.id === theme)!.color;
  useEffect(() => {
    if (!motion) {
      blink.setValue(1);
      jitter.setValue(0);
      return;
    }
    const blinking = Animated.loop(
      Animated.sequence([
        Animated.delay(expression === 'excited' ? 1800 : 3300),
        Animated.timing(blink, {
          toValue: 0.1,
          duration: 80,
          useNativeDriver: true,
        }),
        Animated.timing(blink, {
          toValue: 1,
          duration: 130,
          useNativeDriver: true,
        }),
      ]),
    );
    blinking.start();
    const agitation = Animated.loop(
      Animated.sequence([
        Animated.timing(jitter, {
          toValue: 3,
          duration: 90,
          easing: Easing.inOut(Easing.ease),
          useNativeDriver: true,
        }),
        Animated.timing(jitter, {
          toValue: -3,
          duration: 90,
          useNativeDriver: true,
        }),
      ]),
    );
    if (urgent) agitation.start();
    else jitter.setValue(0);
    return () => {
      blinking.stop();
      agitation.stop();
    };
  }, [blink, jitter, urgent, motion, expression]);
  const radius = theme === 'pixel' || theme === 'robot' ? 4 : 18;
  const height = expression === 'excited' ? 42 : 34;
  const pan = PanResponder.create({
    onStartShouldSetPanResponder: () => true,
    onMoveShouldSetPanResponder: () => true,
    onPanResponderMove: (_, gesture) => {
      if (motion)
        look.setValue({
          x: Math.max(-7, Math.min(7, gesture.dx / 8)),
          y: Math.max(-4, Math.min(4, gesture.dy / 8)),
        });
    },
    onPanResponderRelease: () =>
      Animated.spring(look, {
        toValue: { x: 0, y: 0 },
        useNativeDriver: true,
      }).start(),
    onPanResponderTerminate: () => look.setValue({ x: 0, y: 0 }),
  });
  return (
    <View
      accessibilityLabel={`Mascota ${theme}${
        urgent ? ', reminder apropiat' : ''
      }`}
      {...pan.panHandlers}
      style={[s.hit, compact && s.compact]}
    >
      <Animated.View
        style={[
          s.pair,
          {
            transform: [
              { translateX: jitter },
              ...look.getTranslateTransform(),
            ],
          },
        ]}
      >
        {[0, 1].map(i => (
          <Animated.View
            key={i}
            style={[
              s.eye,
              {
                backgroundColor: color,
                borderRadius: radius,
                height,
                transform: [
                  {
                    scaleY:
                      expression === 'sleepy'
                        ? 0.3
                        : expression === 'wink' && i === 1
                        ? 0.12
                        : blink,
                  },
                ],
              },
            ]}
          >
            {theme === 'cat' && <View style={s.pupil} />}
          </Animated.View>
        ))}
      </Animated.View>
    </View>
  );
}
const s = StyleSheet.create({
  compact: { height: 64 },
  hit: { height: 106, justifyContent: 'center', alignItems: 'center' },
  pair: { flexDirection: 'row', gap: 30 },
  eye: { width: 22, overflow: 'hidden', alignItems: 'center' },
  pupil: { width: 4, height: '100%', backgroundColor: '#10151c' },
});
