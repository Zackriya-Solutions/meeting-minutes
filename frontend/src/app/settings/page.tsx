'use client';

import React from 'react';
import { Bell, User, Lock, Database, Palette, ArrowLeft, Terminal } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { ConsoleToggle } from '@/components/ConsoleToggle';

export default function SettingsPage() {
  const router = useRouter();

  const settingsSections = [
    {
      title: 'Account',
      icon: <User className="w-5 h-5" />,
      items: ['Profile', 'Email', 'Password']
    },
    {
      title: 'Notifications',
      icon: <Bell className="w-5 h-5" />,
      items: ['Email Notifications', 'Push Notifications', 'Meeting Reminders']
    },
    {
      title: 'Privacy',
      icon: <Lock className="w-5 h-5" />,
      items: ['Data Sharing', 'Meeting Access', 'Recording Settings']
    },
    {
      title: 'Storage',
      icon: <Database className="w-5 h-5" />,
      items: ['Storage Usage', 'Auto-delete Settings', 'Backup']
    },
    {
      title: 'Appearance',
      icon: <Palette className="w-5 h-5" />,
      items: ['Theme', 'Font Size', 'Language']
    },
    {
      title: 'Developer',
      icon: <Terminal className="w-5 h-5" />,
      items: ['Console']
    }
  ];

  return (
    <div className="p-8 max-w-4xl mx-auto">
      <div className="flex items-center gap-4 mb-8">
        <button 
          onClick={() => router.back()}
          className="flex items-center gap-2 text-gray-600 hover:text-gray-900 transition-colors"
        >
          <ArrowLeft className="w-5 h-5" />
          <span>Back</span>
        </button>
        <h1 className="text-3xl font-bold">Settings</h1>
      </div>
      
      <div className="space-y-8">
        {settingsSections.map((section) => (
          <div key={section.title} className="border rounded-lg p-6">
            <div className="flex items-center gap-2 mb-4">
              {section.icon}
              <h2 className="text-xl font-semibold">{section.title}</h2>
            </div>
            <div className="space-y-4">
              {section.items.map((item) => (
                <div key={item} className="p-3 hover:bg-gray-50 rounded-md">
                  {item === 'Console' ? (
                    <ConsoleToggle />
                  ) : (
                    <div className="flex items-center justify-between cursor-pointer">
                      <span>{item}</span>
                      <button className="text-blue-600 hover:text-blue-800">Configure</button>
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
