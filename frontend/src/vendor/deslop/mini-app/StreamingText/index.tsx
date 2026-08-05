"use client";

import { useEffect, useState } from 'react';
import { motion, useReducedMotion, type Variants } from 'framer-motion';

import styles from './StreamingText.module.css';

// Vendored from mishanaer/deslop mini-app/components/StreamingText.
// The source uses motion/react-m; this project already ships framer-motion, whose
// motion components and reduced-motion contract are equivalent for this component.
const SPEED_PRESETS = {
  slow: 0.08,
  normal: 0.035,
  fast: 0.015,
} as const;

const TYPE_PER_CHAR = {
  slow: 0.02,
  normal: 0.007,
  fast: 0.007 / 1.5,
} as const;

type StreamingSpeed = keyof typeof SPEED_PRESETS;
type StreamingMode = 'word' | 'char';

const GRAPHEME_SEGMENTER = new Intl.Segmenter();

const splitGraphemes = (text: string) =>
  Array.from(GRAPHEME_SEGMENTER.segment(text), (segment) => segment.segment);

const tokenizeWords = (text: string) =>
  text.split('\n').map((line) =>
    line
      .split(/(\s+)/)
      .filter(Boolean)
      .map((piece) => ({
        content: piece,
        animated: !/^\s+$/.test(piece),
      })),
  );

const wordVariants: Variants = {
  hidden: { opacity: 0, y: 6 },
  visible: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.4, ease: [0.23, 1, 0.32, 1] },
  },
};

const reducedWordVariants: Variants = {
  hidden: { opacity: 0 },
  visible: { opacity: 1, transition: { duration: 0.15 } },
};

interface RevealProps {
  children: string;
  speed: StreamingSpeed;
  delay: number;
  onComplete?: () => void;
}

function WordReveal({ children, speed, delay, onComplete }: RevealProps) {
  const reduceMotion = useReducedMotion();
  const stagger = SPEED_PRESETS[speed] ?? SPEED_PRESETS.normal;
  const lines = tokenizeWords(children);
  const containerVariants: Variants = {
    hidden: {},
    visible: {
      transition: {
        staggerChildren: reduceMotion ? 0 : stagger,
        delayChildren: delay / 1000,
      },
    },
  };
  const variants = reduceMotion ? reducedWordVariants : wordVariants;

  return (
    <motion.span
      className={styles.root}
      initial="hidden"
      animate="visible"
      variants={containerVariants}
      onAnimationComplete={onComplete}
    >
      {lines.map((tokens, lineIndex) => (
        <span key={lineIndex} className={styles.line}>
          {tokens.map((token, tokenIndex) => {
            if (!token.animated) {
              return <span key={tokenIndex}>{token.content}</span>;
            }

            return (
              <motion.span key={tokenIndex} className={styles.token} variants={variants}>
                {token.content}
              </motion.span>
            );
          })}
        </span>
      ))}
    </motion.span>
  );
}

function TypewriterReveal({ children, speed, delay, onComplete }: RevealProps) {
  const reduceMotion = useReducedMotion();
  const perChar = TYPE_PER_CHAR[speed] ?? TYPE_PER_CHAR.normal;
  const graphemes = splitGraphemes(children);
  const total = graphemes.length;
  const [progress, setProgress] = useState(() =>
    reduceMotion ? { whole: total, frac: 0 } : { whole: 0, frac: 0 },
  );

  useEffect(() => {
    if (reduceMotion) {
      setProgress({ whole: total, frac: 0 });
      onComplete?.();
      return undefined;
    }

    const start = performance.now() + delay;
    const charDurationMs = perChar * 1000;
    let frame: number;
    const tick = (now: number) => {
      const elapsed = now - start;
      if (elapsed < 0) {
        frame = requestAnimationFrame(tick);
        return;
      }
      const reveal = elapsed / charDurationMs;
      const whole = Math.min(total, Math.floor(reveal));
      const frac = whole < total ? Math.min(1, reveal - whole) : 0;
      setProgress({ whole, frac });
      if (whole < total) {
        frame = requestAnimationFrame(tick);
      } else {
        onComplete?.();
      }
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [children, delay, onComplete, perChar, reduceMotion, total]);

  const { whole, frac } = progress;
  const leadingChar = whole < total ? graphemes[whole] : null;

  return (
    <span className={styles.typewriter}>
      <span className={styles.typewriterGhost} aria-hidden="true">
        {children}
      </span>
      <span>
        {graphemes.slice(0, whole).join('')}
        {leadingChar !== null && <span style={{ opacity: frac }}>{leadingChar}</span>}
      </span>
    </span>
  );
}

export interface StreamingTextProps {
  children: string;
  speed?: StreamingSpeed;
  mode?: StreamingMode;
  delay?: number;
  replayKey?: string | number;
  onComplete?: () => void;
}

export default function StreamingText({
  children,
  speed = 'fast',
  mode = 'word',
  delay = 0,
  replayKey,
  onComplete,
}: StreamingTextProps) {
  const Component = mode === 'char' ? TypewriterReveal : WordReveal;
  return (
    <Component
      key={replayKey}
      speed={speed}
      delay={delay}
      onComplete={onComplete}
    >
      {children}
    </Component>
  );
}
