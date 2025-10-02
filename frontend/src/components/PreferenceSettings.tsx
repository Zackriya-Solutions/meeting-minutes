"use client"

import { useEffect, useState } from "react"
import { Switch } from "./ui/switch"
import { invoke } from "@tauri-apps/api/core"

export function PreferenceSettings() {
  const [notificationsEnabled, setNotificationsEnabled] = useState<boolean | null>(null);

  useEffect(() => {
    const savedPreference = localStorage.getItem('notificationsEnabled')
    setNotificationsEnabled(savedPreference !== null ? savedPreference === 'true' : true);
  }, [])

  useEffect(() => {
    if (notificationsEnabled === null) return;

    localStorage.setItem('notificationsEnabled', String(notificationsEnabled));
    console.log("Seeting notificationsEnabled", notificationsEnabled )
    invoke('set_notification_enabled', { enabled: notificationsEnabled })
  }, [notificationsEnabled])

  if (notificationsEnabled === null) {
    return <div className="max-w-2xl mx-auto p-6">Loading Preferences...</div>
  }

  return (
    <div className="max-w-2xl mx-auto space-y-8">
      <div className="flex items-center justify-between bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <div className="">
          <h3 className="text-lg font-semibold text-gray-900 mb-2">Notification</h3>
          <p className="text-sm text-gray-600 mb-4">Enable or disable notifications of start and end of meeting</p>
        </div>
        <Switch checked={notificationsEnabled} onCheckedChange={setNotificationsEnabled} />
      </div>
    </div>
  )
}
