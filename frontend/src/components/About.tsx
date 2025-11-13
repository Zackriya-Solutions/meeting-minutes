import React from "react";
import { invoke } from '@tauri-apps/api/core';
import Image from 'next/image';
import AnalyticsConsentSwitch from "./AnalyticsConsentSwitch";
import { useTranslation } from 'react-i18next';


export function About() {
    const { t } = useTranslation('common');

    const handleContactClick = async () => {
        try {
            await invoke('open_external_url', { url: 'https://meetily.zackriya.com/#about' });
        } catch (error) {
            console.error('Failed to open link:', error);
        }
    };

    return (
        <div className="p-4 space-y-4 h-[80vh] overflow-y-auto">
            {/* Compact Header */}
            <div className="text-center">
                <div className="mb-3">
                    <Image
                        src="icon_128x128.png"
                        alt="Meetily Logo"
                        width={64}
                        height={64}
                        className="mx-auto"
                    />
                </div>
                {/* <h1 className="text-xl font-bold text-gray-900">Meetily</h1> */}
                <span className="text-sm text-gray-500"> {t('about.version')}</span>
                <p className="text-medium text-gray-600 mt-1">
                    {t('about.tagline')}
                </p>
            </div>

            {/* Features Grid - Compact */}
            <div className="space-y-3">
                <h2 className="text-base font-semibold text-gray-800">{t('about.whatMakesDifferent')}</h2>
                <div className="grid grid-cols-2 gap-2">
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">{t('about.privacyFirst')}</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">{t('about.privacyFirstDescription')}</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">{t('about.useAnyModel')}</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">{t('about.useAnyModelDescription')}</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">{t('about.costSmart')}</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">{t('about.costSmartDescription')}</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">{t('about.worksEverywhere')}</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">{t('about.worksEverywhereDescription')}</p>
                    </div>
                </div>
            </div>

            {/* Coming Soon - Compact */}
            <div className="bg-blue-50 rounded p-3">
                <p className="text-s text-blue-800">
                    <span className="font-bold">{t('about.comingSoon')}</span> {t('about.comingSoonDescription')}
                </p>
            </div>

            {/* CTA Section - Compact */}
            <div className="text-center space-y-2">
                <h3 className="text-medium font-semibold text-gray-800">{t('about.readyToPushFurther')}</h3>
                <p className="text-s text-gray-600">
                    {t('about.customAIDescription')}
                </p>
                <button
                    onClick={handleContactClick}
                    className="inline-flex items-center px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium rounded transition-colors duration-200 shadow-sm hover:shadow-md"
                >
                    {t('about.chatWithTeam')}
                </button>
            </div>

            {/* Footer - Compact */}
            <div className="pt-2 border-t border-gray-200 text-center">
                <p className="text-xs text-gray-400">
                    {t('about.builtBy')}
                </p>
            </div>
            <AnalyticsConsentSwitch />
        </div>

    )
}