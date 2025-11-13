'use client'

import React, { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Globe } from 'lucide-react'

/**
 * Language Selector Component
 *
 * Allows users to switch between Portuguese and English for AI-generated summaries.
 * Syncs with backend language preference and updates i18n context.
 *
 * Date: 13/11/2025 - Author: Luiz
 */
export function LanguageSelector() {
  const { t, i18n } = useTranslation('common')
  const [language, setLanguage] = useState<string>('pt')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)

  // Load current language preference on mount
  useEffect(() => {
    const loadLanguage = async () => {
      try {
        const currentLang = await invoke<string>('api_get_language')
        setLanguage(currentLang)
        i18n.changeLanguage(currentLang)
      } catch (error) {
        console.error('Failed to load language preference:', error)
        toast.error(t('errors.loadFailed'))
      } finally {
        setLoading(false)
      }
    }

    loadLanguage()
  }, [i18n, t])

  const handleLanguageChange = async (newLanguage: string) => {
    setSaving(true)
    try {
      // Save to backend
      await invoke('api_set_language', { language: newLanguage })

      // Update local state
      setLanguage(newLanguage)

      // Update i18n
      await i18n.changeLanguage(newLanguage)

      // Show success toast
      toast.success(t('notifications.success.languageChanged'))
    } catch (error) {
      console.error('Failed to save language preference:', error)
      toast.error(t('notifications.error.saveFailed'))
    } finally {
      setSaving(false)
    }
  }

  if (loading) {
    return (
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <Globe className="w-4 h-4 text-muted-foreground" />
          <h3 className="text-sm font-medium">{t('common.loading')}</h3>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <Globe className="w-4 h-4 text-muted-foreground" />
        <h3 className="text-sm font-medium">{t('settings.language.title')}</h3>
      </div>
      <p className="text-xs text-muted-foreground">
        {t('settings.language.description')}
      </p>
      <Select
        value={language}
        onValueChange={handleLanguageChange}
        disabled={saving}
      >
        <SelectTrigger className="w-full">
          <SelectValue placeholder={t('settings.language.title')} />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="pt">
            🇧🇷 {t('settings.language.portuguese')}
          </SelectItem>
          <SelectItem value="en">
            🇺🇸 {t('settings.language.english')}
          </SelectItem>
        </SelectContent>
      </Select>
      {saving && (
        <p className="text-xs text-muted-foreground">{t('common.loading')}</p>
      )}
    </div>
  )
}
