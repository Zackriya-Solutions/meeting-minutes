'use client';

import React from 'react';
import { X, Info, Shield } from '@/components/memento/LucideCompat';

interface AnalyticsDataModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirmDisable: () => void;
}

export default function AnalyticsDataModal({ isOpen, onClose, onConfirmDisable }: AnalyticsDataModalProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-[var(--bg-canvas)] rounded-lg shadow-none max-w-2xl w-full mx-4 max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-[var(--border-subtle)]">
          <div className="flex items-center gap-3">
            <Shield className="w-6 h-6 text-[var(--gold)]" />
            <h2 className="text-xl font-semibold text-[var(--fg1)]">Какие данные собирает аналитика</h2>
          </div>
          <button
            onClick={onClose}
            className="text-[var(--fg3)] hover:text-[var(--fg2)] transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-6 space-y-6">
          {/* Privacy Notice */}
          <div className="bg-[color-mix(in_srgb,var(--success)_12%,transparent)] border border-[color-mix(in_srgb,var(--success)_42%,transparent)] rounded-lg p-4">
            <div className="flex items-start gap-3">
              <Info className="w-5 h-5 text-[var(--success)] mt-0.5 flex-shrink-0" />
              <div className="text-sm text-[var(--success)]">
                <p className="font-semibold mb-1">Содержимое встреч не отправляется</p>
                <p>Аналитика выключена по умолчанию. После включения отправляются только <strong>обезличенные данные об использовании</strong>. Содержимое встреч, названия, пути к файлам и личные данные не собираются.</p>
              </div>
            </div>
          </div>

          {/* Data Categories */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold text-[var(--fg1)]">Что отправляется после включения:</h3>

            {/* Model Preferences */}
            <div className="border border-[var(--border-subtle)] rounded-lg p-4">
              <h4 className="font-semibold text-[var(--fg1)] mb-2">1. Выбранные модели</h4>
              <ul className="text-sm text-[var(--fg2)] space-y-1 ml-4">
                <li>• Модель расшифровки (e.g., "Whisper large-v3", "Parakeet")</li>
                <li>• Модель для создания сути (e.g., "Llama 3.2", "Claude Sonnet")</li>
                <li>• Провайдер модели (e.g., "Local", "Ollama", "OpenRouter")</li>
              </ul>
              <p className="text-xs text-[var(--fg2)] mt-2 italic">Помогает понять, какие модели чаще выбирают</p>
            </div>

            {/* Meeting Metrics */}
            <div className="border border-[var(--border-subtle)] rounded-lg p-4">
              <h4 className="font-semibold text-[var(--fg1)] mb-2">2. Обезличенные показатели встреч</h4>
              <ul className="text-sm text-[var(--fg2)] space-y-1 ml-4">
                <li>• Длительность записи (e.g., "125 seconds")</li>
                <li>• Длительность пауз (e.g., "5 seconds")</li>
                <li>• Количество фрагментов расшифровки</li>
                <li>• Количество обработанных аудиофрагментов</li>
              </ul>
              <p className="text-xs text-[var(--fg2)] mt-2 italic">Помогает улучшать скорость и стабильность</p>
            </div>

            {/* Device Types */}
            <div className="border border-[var(--border-subtle)] rounded-lg p-4">
              <h4 className="font-semibold text-[var(--fg1)] mb-2">3. Типы устройств без названий</h4>
              <ul className="text-sm text-[var(--fg2)] space-y-1 ml-4">
                <li>• Тип микрофона: "Bluetooth" or "Wired" or "Unknown"</li>
                <li>• Тип системного звука: "Bluetooth" or "Wired" or "Unknown"</li>
              </ul>
              <p className="text-xs text-[var(--fg2)] mt-2 italic">Помогает улучшать совместимость без передачи названий устройств</p>
            </div>

            {/* Usage Patterns */}
            <div className="border border-[var(--border-subtle)] rounded-lg p-4">
              <h4 className="font-semibold text-[var(--fg1)] mb-2">4. Использование приложения</h4>
              <ul className="text-sm text-[var(--fg2)] space-y-1 ml-4">
                <li>• Запуск и завершение приложения</li>
                <li>• Длительность сессии</li>
                <li>• Использование функций (e.g., "settings changed")</li>
                <li>• Возникновение ошибок</li>
              </ul>
              <p className="text-xs text-[var(--fg2)] mt-2 italic">Помогает улучшать работу приложения</p>
            </div>

            {/* Platform Info */}
            <div className="border border-[var(--border-subtle)] rounded-lg p-4">
              <h4 className="font-semibold text-[var(--fg1)] mb-2">5. Информация о платформе</h4>
              <ul className="text-sm text-[var(--fg2)] space-y-1 ml-4">
                <li>• Операционная система (e.g., "macOS", "Windows")</li>
                <li>• Версия приложения</li>
                <li>• Архитектура (e.g., "x86_64", "aarch64")</li>
              </ul>
              <p className="text-xs text-[var(--fg2)] mt-2 italic">Помогает планировать поддержку платформ</p>
            </div>
          </div>

          {/* What We DON'T Collect */}
          <div className="bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] border border-[color-mix(in_srgb,var(--danger)_42%,transparent)] rounded-lg p-4">
            <h4 className="font-semibold text-[var(--danger)] mb-2">Что не собирается:</h4>
            <ul className="text-sm text-[var(--danger)] space-y-1 ml-4">
              <li>• Названия встреч</li>
              <li>• Имена файлов, пути и папки встреч</li>
              <li>• Расшифровки и содержимое встреч</li>
              <li>• Аудиозаписи</li>
              <li>• Названия устройств</li>
              <li>• Личные данные</li>
              <li>• Данные, позволяющие определить пользователя</li>
            </ul>
          </div>

          {/* Example Event */}
          <div className="bg-[var(--bg-sheet)] border border-[var(--border-subtle)] rounded-lg p-4">
            <h4 className="font-semibold text-[var(--fg1)] mb-2">Пример события:</h4>
            <pre className="text-xs text-[var(--fg2)] overflow-x-auto">
              {`{
  "event": "meeting_ended",
  "app_version": "0.4.0",
  "transcription_provider": "parakeet",
  "transcription_model": "parakeet-tdt-0.6b-v3-int8",
  "summary_provider": "ollama",
  "summary_model": "llama3.2:latest",
  "total_duration_seconds": "125.5",
  "microphone_device_type": "Wired",
  "system_audio_device_type": "Bluetooth",
  "chunks_processed": "150",
  "had_fatal_error": "false"
}`}
            </pre>
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between gap-4 p-6 border-t border-[var(--border-subtle)] bg-[var(--bg-sheet)]">
          <button
            onClick={onClose}
            className="px-4 py-2 text-[var(--fg2)] bg-[var(--bg-canvas)] border border-[var(--border-strong)] rounded-md hover:bg-[var(--bg-sheet)] transition-colors"
          >
            Оставить аналитику
          </button>
          <button
            onClick={onConfirmDisable}
            className="px-4 py-2 text-[var(--fg-inverse)] bg-[var(--danger)] rounded-md hover:opacity-90 transition-colors"
          >
            Выключить аналитику
          </button>
        </div>
      </div>
    </div>
  );
}
