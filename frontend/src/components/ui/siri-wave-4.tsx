"use client"

import { useEffect, useRef, type HTMLAttributes } from "react"

import { cn } from "@/lib/utils"

type SiriWave4Props = HTMLAttributes<HTMLDivElement> & {
  active?: boolean
  processing?: boolean
  height?: number
  sensitivity?: number
}

const BASE_AMPLITUDE = 0.32

function easeInOutCubic(value: number) {
  const clamped = Math.max(0, Math.min(1, value))
  return clamped < 0.5
    ? 4 * clamped * clamped * clamped
    : 1 - Math.pow(-2 * clamped + 2, 3) / 2
}

/**
 * Canvas adaptation of the layered visual language used by Wave 4 in
 * nilotic/SiriWave. The original project is SwiftUI-only, so this component
 * implements the same product behavior for the webview without embedding its
 * source: six travelling curves, a soft central envelope and live mic power.
 * Reference: https://github.com/nilotic/SiriWave
 */
export function SiriWave4({
  active = false,
  processing = false,
  height = 64,
  sensitivity = 1.35,
  className,
  ...props
}: SiriWave4Props) {
  const containerRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const analyserRef = useRef<AnalyserNode | null>(null)
  const audioContextRef = useRef<AudioContext | null>(null)
  const streamRef = useRef<MediaStream | null>(null)
  const amplitudeRef = useRef(BASE_AMPLITUDE)
  const phaseRef = useRef(0)

  useEffect(() => {
    const container = containerRef.current
    const canvas = canvasRef.current
    if (!container || !canvas) return

    const resize = () => {
      const rect = container.getBoundingClientRect()
      const dpr = window.devicePixelRatio || 1
      canvas.width = Math.max(1, Math.round(rect.width * dpr))
      canvas.height = Math.max(1, Math.round(rect.height * dpr))
      canvas.style.width = `${rect.width}px`
      canvas.style.height = `${rect.height}px`
    }

    const observer = new ResizeObserver(resize)
    observer.observe(container)
    resize()
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    if (!active) return

    let cancelled = false

    const connectMicrophone = async () => {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({
          audio: {
            echoCancellation: true,
            noiseSuppression: true,
            autoGainControl: true,
          },
        })
        if (cancelled) {
          stream.getTracks().forEach((track) => track.stop())
          return
        }

        const AudioContextConstructor = window.AudioContext
          || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext
        const audioContext = new AudioContextConstructor()
        const analyser = audioContext.createAnalyser()
        analyser.fftSize = 256
        analyser.smoothingTimeConstant = 0.84
        audioContext.createMediaStreamSource(stream).connect(analyser)

        streamRef.current = stream
        audioContextRef.current = audioContext
        analyserRef.current = analyser
      } catch {
        // The native recorder may already own the device. Keep the wave alive
        // with a restrained synthetic pulse instead of showing a broken state.
        analyserRef.current = null
      }
    }

    void connectMicrophone()

    return () => {
      cancelled = true
      streamRef.current?.getTracks().forEach((track) => track.stop())
      streamRef.current = null
      analyserRef.current = null
      if (audioContextRef.current?.state !== "closed") {
        void audioContextRef.current?.close()
      }
      audioContextRef.current = null
    }
  }, [active])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const context = canvas.getContext("2d")
    if (!context) return

    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches
    const audioData = new Uint8Array(128)
    let frame = 0
    let lastTime = performance.now()

    const render = (now: number) => {
      const dpr = window.devicePixelRatio || 1
      const width = canvas.width / dpr
      const canvasHeight = canvas.height / dpr
      const delta = Math.min(40, now - lastTime)
      lastTime = now

      let targetAmplitude = BASE_AMPLITUDE
      if (active && analyserRef.current) {
        analyserRef.current.getByteTimeDomainData(audioData)
        let energy = 0
        for (const sample of audioData) {
          const centered = (sample - 128) / 128
          energy += centered * centered
        }
        const voiceLevel = Math.sqrt(energy / audioData.length) * 6.2 * sensitivity
        const easedVoiceLevel = easeInOutCubic(voiceLevel)
        targetAmplitude = BASE_AMPLITUDE + easedVoiceLevel * (1 - BASE_AMPLITUDE)
      } else if (active) {
        targetAmplitude = BASE_AMPLITUDE + Math.sin(now / 430) * 0.055 + Math.sin(now / 970) * 0.04
      } else if (processing) {
        targetAmplitude = 0.26 + Math.sin(now / 360) * 0.045
      }

      const easingDuration = targetAmplitude > amplitudeRef.current ? 110 : 260
      const easingProgress = 1 - Math.exp(-delta / easingDuration)
      amplitudeRef.current += (targetAmplitude - amplitudeRef.current) * easingProgress
      if (!reduceMotion) {
        phaseRef.current -= delta * (0.0019 + amplitudeRef.current * 0.0014)
      }

      context.setTransform(dpr, 0, 0, dpr, 0, 0)
      context.clearRect(0, 0, width, canvasHeight)
      const styles = getComputedStyle(canvas)
      const fallbackColor = styles.color
      const layerColors = [
        styles.getPropertyValue("--deslop-primary-50").trim() || fallbackColor,
        styles.getPropertyValue("--deslop-primary-30").trim() || fallbackColor,
        styles.getPropertyValue("--deslop-primary-20").trim() || fallbackColor,
        styles.getPropertyValue("--deslop-primary-10").trim() || fallbackColor,
        styles.getPropertyValue("--deslop-primary-8").trim() || fallbackColor,
        styles.getPropertyValue("--deslop-primary-5").trim() || fallbackColor,
      ]
      context.lineCap = "round"
      context.lineJoin = "round"

      const centerY = canvasHeight / 2
      const maxAmplitude = Math.max(1, canvasHeight * 0.47)
      const layerScales = [1, 0.7, 0.41, 0.12, -0.27, -0.57]

      layerScales.forEach((layerScale, index) => {
        context.beginPath()
        context.strokeStyle = layerColors[index]
        context.lineWidth = index === 0 ? 1.5 : 0.75
        context.globalAlpha = 1

        for (let x = 0; x <= width; x += 2) {
          const progress = width > 0 ? x / width : 0
          const normalized = progress * 2 - 1
          const envelope = Math.pow(Math.max(0, 1 - normalized * normalized), 1.65)
          const frequency = 1.5 + index * 0.018
          const y = centerY
            + Math.sin(Math.PI * 2 * frequency * progress + phaseRef.current + index * 0.11)
            * maxAmplitude
            * amplitudeRef.current
            * layerScale
            * envelope

          if (x === 0) context.moveTo(x, y)
          else context.lineTo(x, y)
        }

        context.stroke()
      })

      context.globalAlpha = 1
      frame = requestAnimationFrame(render)
    }

    frame = requestAnimationFrame(render)
    return () => cancelAnimationFrame(frame)
  }, [active, processing, sensitivity])

  return (
    <div
      ref={containerRef}
      className={cn("relative w-full", className)}
      style={{ height }}
      role="img"
      aria-label={active ? "Live audio waveform" : "Audio waveform"}
      {...props}
    >
      <canvas ref={canvasRef} className="block size-full" aria-hidden="true" />
    </div>
  )
}
