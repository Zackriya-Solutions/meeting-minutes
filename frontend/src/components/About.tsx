import React, { useState, useEffect } from "react";
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import Image from 'next/image';
import AnalyticsConsentSwitch from "./AnalyticsConsentSwitch";
import { UpdateDialog } from "./UpdateDialog";
import { updateService, UpdateInfo } from '@/services/updateService';
import { Button } from './ui/button';
import { Loader2, CheckCircle2 } from '@/components/deslop-icons';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';


export function About() {
    const t = useT();
    const [currentVersion, setCurrentVersion] = useState<string>('0.4.0');
    const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
    const [isChecking, setIsChecking] = useState(false);
    const [showUpdateDialog, setShowUpdateDialog] = useState(false);

    useEffect(() => {
        // Get current version on mount
        getVersion().then(setCurrentVersion).catch(console.error);
    }, []);

    const handleContactClick = async () => {
        try {
            await invoke('open_external_url', { url: 'https://github.com/andyzt/meet_at_giga/issues' });
        } catch (error) {
            console.error('Failed to open link:', error);
        }
    };

    const handleCheckForUpdates = async () => {
        setIsChecking(true);
        try {
            const info = await updateService.checkForUpdates(true);
            setUpdateInfo(info);
            if (info.available) {
                setShowUpdateDialog(true);
            } else {
                toast.success(t('You are running the latest version'));
            }
        } catch (error: any) {
            console.error('Failed to check for updates:', error);
            toast.error(t('Failed to check for updates: ') + (error.message || t('Unknown error')));
        } finally {
            setIsChecking(false);
        }
    };

    return (
        <div className="p-4 space-y-4 h-[80vh] overflow-y-auto">
            {/* Compact Header */}
            <div className="text-center">
                <div className="mb-3">
                    <Image
                        src="/memento-app-icon.png"
                        alt={t('Memento Logo')}
                        width={64}
                        height={64}
                        className="mx-auto"
                    />
                </div>
                <h1 className="text-xl font-semibold">memento</h1>
                <span className="text-sm text-muted-foreground"> v{currentVersion}</span>
                <p className="text-medium text-muted-foreground mt-1">
                    {t('Real-time notes and summaries that never leave your machine.')}
                </p>
                <div className="mt-3">
                    <Button
                        onClick={handleCheckForUpdates}
                        disabled={isChecking}
                        variant="outline"
                        size="sm"
                        className="text-xs"
                    >
                        {isChecking ? (
                            <>
                                <Loader2 className="h-3 w-3 mr-2 animate-spin" />
                                {t('Checking...')}
                            </>
                        ) : (
                            <>
                                <CheckCircle2 className="h-3 w-3 mr-2" />
                                {t('Check for Updates')}
                            </>
                        )}
                    </Button>
                    {updateInfo?.available && (
                        <div className="mt-2 text-xs text-primary">
                            {t('Update available: v')}{updateInfo.version}
                        </div>
                    )}
                </div>
            </div>

            {/* Features Grid - Compact */}
            <div className="space-y-3">
                <h2 className="text-base font-semibold text-foreground">{t('What makes Memento different')}</h2>
                <div className="grid grid-cols-2 gap-2">
                    <div className="bg-background rounded p-3 hover:bg-muted transition-colors">
                        <h3 className="font-bold text-sm text-foreground mb-1">{t('Privacy-first')}</h3>
                        <p className="text-xs text-muted-foreground leading-relaxed">{t('Your data & AI processing workflow can now stay within your premise. No cloud, no leaks.')}</p>
                    </div>
                    <div className="bg-background rounded p-3 hover:bg-muted transition-colors">
                        <h3 className="font-bold text-sm text-foreground mb-1">{t('Use Any Model')}</h3>
                        <p className="text-xs text-muted-foreground leading-relaxed">{t('Prefer local open-source model? Great. Want to plug in an external API? Also fine. No lock-in.')}</p>
                    </div>
                    <div className="bg-background rounded p-3 hover:bg-muted transition-colors">
                        <h3 className="font-bold text-sm text-foreground mb-1">{t('Cost-Smart')}</h3>
                        <p className="text-xs text-muted-foreground leading-relaxed">{t('Avoid pay-per-minute bills by running models locally (or pay only for the calls you choose).')}</p>
                    </div>
                    <div className="bg-background rounded p-3 hover:bg-muted transition-colors">
                        <h3 className="font-bold text-sm text-foreground mb-1">{t('Works everywhere')}</h3>
                        <p className="text-xs text-muted-foreground leading-relaxed">{t('Google Meet, Zoom, Teams-online or offline.')}</p>
                    </div>
                </div>
            </div>

            {/* Coming Soon - Compact */}
            <div className="bg-primary/10 rounded p-3">
                <p className="text-s text-primary">
                    <span className="font-bold">{t('Coming soon:')}</span> {t('A library of on-device AI agents-automating follow-ups, action tracking, and more.')}
                </p>
            </div>

            {/* CTA Section - Compact */}
            <div className="text-center space-y-2">
                <h3 className="text-medium font-semibold text-foreground">{t('Ready to push your business further?')}</h3>
                <p className="text-s text-muted-foreground">
                    {t("If you're planning to build privacy-first custom AI agents or a fully tailored product for your business, we can help you build it.")}
                </p>
                <button
                    onClick={handleContactClick}
                    className="inline-flex items-center px-4 py-2 bg-primary hover:bg-primary/90 text-primary-foreground text-sm font-medium rounded transition-colors duration-200 shadow-none hover:shadow-none"
                >
                    {t('Chat with the Zackriya team')}
                </button>
            </div>

            {/* Footer - Compact */}
            <div className="pt-2 border-t border-border text-center">
                <p className="text-xs text-muted-foreground">
                    {t('Built by Zackriya Solutions')}
                </p>
            </div>
            <AnalyticsConsentSwitch />

            {/* Update Dialog */}
            <UpdateDialog
                open={showUpdateDialog}
                onOpenChange={setShowUpdateDialog}
                updateInfo={updateInfo}
            />
        </div>

    )
}
