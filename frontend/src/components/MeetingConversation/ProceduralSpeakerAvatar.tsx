"use client";

import { useEffect, useRef, type CSSProperties } from 'react';
import {
  createAvatar,
  type AvatarController,
  type AvatarDefinition,
} from '@bible-strong/avatar-web';
import avatarLabData from './avatarLabData.json';
import { cn } from '@/lib/utils';

export type TranscriptReaction =
  | 'neutral'
  | 'excited'
  | 'bored'
  | 'suspicious'
  | 'angry'
  | 'drowsy'
  | 'happy'
  | 'curious'
  | 'confused'
  | 'surprised'
  | 'proud'
  | 'shy'
  | 'sad'
  | 'laughing'
  | 'scared'
  | 'playful'
  | 'celebrate';

type StudioExpression = (typeof avatarLabData.expressions)[number];
type StudioAvatar = (typeof avatarLabData.avatars)[number];

const DEFAULT_EYES = {
  widthLeft: 20,
  widthRight: 20,
  heightLeft: 50,
  heightRight: 50,
  spacing: 35,
  positionXLeft: 0,
  positionXRight: 0,
  positionYLeft: -7,
  positionYRight: -7,
  leftAngle: 0,
  rightAngle: 0,
} as const;

const surfaceDefinition = (surface: StudioAvatar['body']['primary']) => ({
  type: surface.type,
  width: surface.width,
  height: surface.height,
  depth: surface.depth,
  roundness: surface.roundness,
  ...('morphRoundness' in surface ? { morphRoundness: surface.morphRoundness } : {}),
  ...('tipRoundness' in surface ? { tipRoundness: surface.tipRoundness } : {}),
  ...('baseRoundness' in surface ? { baseRoundness: surface.baseRoundness } : {}),
});

function withAvatarEyes(expression: StudioExpression, avatar: StudioAvatar): StudioExpression {
  const adjusted = { ...expression };
  (Object.keys(DEFAULT_EYES) as Array<keyof typeof DEFAULT_EYES>).forEach((field) => {
    adjusted[field] = expression[field] + avatar.eyes[field] - DEFAULT_EYES[field];
  });
  return adjusted;
}

function expressionDefinition(expression: StudioExpression) {
  const bodyColor = 'bodyColor' in expression ? expression.bodyColor : undefined;
  const eyeColor = expression.semanticKey === 'angry-brows'
    ? bodyColor
    : ('eyeColor' in expression ? expression.eyeColor : undefined);
  const colors = {
    ...(bodyColor ? { body: bodyColor } : {}),
    ...(eyeColor ? { eyes: eyeColor } : {}),
  };

  return {
    head: { x: expression.headX, y: expression.headY, z: expression.headZ },
    eyes: {
      left: {
        width: expression.widthLeft,
        height: expression.heightLeft,
        x: expression.positionXLeft,
        y: expression.positionYLeft,
        angle: expression.leftAngle,
      },
      right: {
        width: expression.widthRight,
        height: expression.heightRight,
        x: expression.positionXRight,
        y: expression.positionYRight,
        angle: expression.rightAngle,
      },
      spacing: expression.spacing,
    },
    perspective: expression.perspective,
    motion: { eyes: expression.eyeMotion, body: expression.bodyMotion },
    ...(Object.keys(colors).length > 0 ? { colors } : {}),
  };
}

const neutralExpression = (avatar: StudioAvatar): StudioExpression => ({
  id: 'expression-neutral',
  semanticKey: 'neutral-placeholder',
  headX: 0,
  headY: 0,
  headZ: 0,
  ...avatar.eyes,
  perspective: 1,
  eyeMotion: 'none',
  bodyMotion: 'none',
});

function createDefinition(avatar: StudioAvatar): AvatarDefinition {
  const expressionKeyById = new Map(
    avatarLabData.expressions.map((expression) => [expression.id, expression.semanticKey]),
  );
  const expressions = Object.fromEntries([
    ['neutral', expressionDefinition(neutralExpression(avatar))],
    ...avatarLabData.expressions.map((expression) => [
      expression.semanticKey,
      expressionDefinition(withAvatarEyes(expression, avatar)),
    ]),
  ]);
  const animations = Object.fromEntries(
    avatarLabData.animations.map((animation) => [
      animation.semanticKey,
      {
        playbackMode: animation.playbackMode,
        steps: animation.steps.map((step) => ({
          expression: expressionKeyById.get(step.expressionId)!,
          holdMs: step.holdMs,
          transitionMs: step.transitionMs,
          transition: step.transition,
        })),
        blink: animation.blink,
        metadata: {
          label: animation.name,
          description: animation.description,
          group: animation.group,
        },
      },
    ]),
  );

  return {
    schema: 'bible-strong/avatar-definition',
    schemaVersion: 1,
    name: avatar.name,
    body: {
      primary: surfaceDefinition(avatar.body.primary),
      nodes: avatar.body.nodes.map((node) => ({
        ...('layer' in node ? { layer: node.layer } : {}),
        surface: surfaceDefinition(node.surface as StudioAvatar['body']['primary']),
        position: node.position as [number, number, number],
        rotation: node.rotation as [number, number, number],
      })),
    },
    colors: avatar.colors,
    expressions,
    expressionOrder: ['neutral', ...avatarLabData.expressions.map(({ semanticKey }) => semanticKey)],
    animations,
    animationOrder: avatarLabData.animations.map(({ semanticKey }) => semanticKey),
  } as AvatarDefinition;
}

const definitions = avatarLabData.avatars.map(createDefinition);
const sphereAvatarIndex = avatarLabData.avatars.findIndex(({ id }) => id === 'primitive-sphere');
const participantAvatarIndices = avatarLabData.avatars
  .map((avatar, index) => ({ avatar, index }))
  .filter(({ avatar }) => avatar.id !== 'primitive-sphere')
  .map(({ index }) => index);

if (sphereAvatarIndex < 0 || participantAvatarIndices.length === 0) {
  throw new Error('Avatar Lab Sphere preset is missing');
}

function stableParticipantAvatarIndex(speakerId: number): number {
  const stableIndex = ((Math.trunc(speakerId) % participantAvatarIndices.length)
    + participantAvatarIndices.length) % participantAvatarIndices.length;
  return participantAvatarIndices[stableIndex];
}

export function ProceduralSpeakerAvatar({
  speakerId,
  reaction = 'neutral',
  preset = 'participant',
  strokeWidth = 3,
  className,
}: {
  speakerId: number;
  reaction?: TranscriptReaction;
  preset?: 'participant' | 'sphere';
  strokeWidth?: number;
  className?: string;
}) {
  const mountRef = useRef<HTMLSpanElement>(null);
  const controllerRef = useRef<AvatarController | null>(null);
  const isVisibleRef = useRef(true);
  const animationRef = useRef('idle');
  const avatarIndex = preset === 'sphere'
    ? sphereAvatarIndex
    : stableParticipantAvatarIndex(speakerId);
  const definition = definitions[avatarIndex];
  const animation = reaction === 'neutral' ? 'idle' : reaction;
  const color = avatarLabData.avatars[avatarIndex].colors.body;

  animationRef.current = animation;

  useEffect(() => {
    const mount = mountRef.current;
    if (!mount) return;
    const controller = createAvatar(mount, {
      definition,
      defaultAnimation: animation,
      size: '100%',
      ariaLabel: `${definition.name ?? 'Avatar'}: ${animation}`,
      className: 'avatar-lab-runtime__canvas',
    });
    controllerRef.current = controller;
    const observer = typeof IntersectionObserver === 'undefined'
      ? null
      : new IntersectionObserver(([entry]) => {
        isVisibleRef.current = entry.isIntersecting;
        if (entry.isIntersecting) {
          controller.play(animationRef.current);
        } else {
          controller.pause();
        }
      });
    observer?.observe(mount);
    return () => {
      observer?.disconnect();
      controller.destroy();
      controllerRef.current = null;
    };
  }, [definition]);

  useEffect(() => {
    if (isVisibleRef.current) controllerRef.current?.play(animation);
  }, [animation]);

  return (
    <span
      ref={mountRef}
      className={cn('avatar-lab-runtime block h-full w-full', className)}
      data-animation={animation}
      style={{
        color,
        '--memento-avatar-stroke-width': `${strokeWidth}px`,
      } as CSSProperties}
    />
  );
}
