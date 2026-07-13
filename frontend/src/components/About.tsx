import React, { useState, useEffect } from "react";
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import Image from 'next/image';
import AnalyticsConsentSwitch from "./AnalyticsConsentSwitch";
import { UpdateDialog } from "./UpdateDialog";
import { updateService, UpdateInfo } from '@/services/updateService';
import { Button } from './ui/button';
import { Loader2, CheckCircle2 } from '@/components/memento/LucideCompat';
import { toast } from 'sonner';


export function About() {
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
                toast.success('Установлена последняя версия');
            }
        } catch (error: any) {
            console.error('Failed to check for updates:', error);
            toast.error('Не удалось проверить обновления: ' + (error.message || 'неизвестная ошибка'));
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
                        alt="Memento logo"
                        width={64}
                        height={64}
                        className="mx-auto"
                    />
                </div>
                <h1 className="text-xl font-semibold">memento</h1>
                <span className="text-sm text-[var(--fg2)]"> v{currentVersion}</span>
                <p className="text-medium text-[var(--fg2)] mt-1">
                    Встречи, расшифровки и суть остаются на твоём устройстве.
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
                                Проверяем…
                            </>
                        ) : (
                            <>
                                <CheckCircle2 className="h-3 w-3 mr-2" />
                                Проверить обновления
                            </>
                        )}
                    </Button>
                    {updateInfo?.available && (
                        <div className="mt-2 text-xs text-[var(--gold)]">
                            Доступна версия {updateInfo.version}
                        </div>
                    )}
                </div>
            </div>

            {/* Features Grid - Compact */}
            <div className="space-y-3">
                <h2 className="text-base font-semibold text-[var(--fg1)]">Почему Memento</h2>
                <div className="grid grid-cols-2 gap-2">
                    <div className="bg-[var(--bg-sheet)] rounded p-3 hover:bg-[var(--bg-elevated)] transition-colors">
                        <h3 className="font-bold text-sm text-[var(--fg1)] mb-1">Локальные данные</h3>
                        <p className="text-xs text-[var(--fg2)] leading-relaxed">Записи и обработка могут полностью оставаться на твоём устройстве.</p>
                    </div>
                    <div className="bg-[var(--bg-sheet)] rounded p-3 hover:bg-[var(--bg-elevated)] transition-colors">
                        <h3 className="font-bold text-sm text-[var(--fg1)] mb-1">Любая модель</h3>
                        <p className="text-xs text-[var(--fg2)] leading-relaxed">Используй локальные модели или подключи выбранного облачного провайдера.</p>
                    </div>
                    <div className="bg-[var(--bg-sheet)] rounded p-3 hover:bg-[var(--bg-elevated)] transition-colors">
                        <h3 className="font-bold text-sm text-[var(--fg1)] mb-1">Без оплаты за минуты</h3>
                        <p className="text-xs text-[var(--fg2)] leading-relaxed">Локальные модели не требуют подписки или поминутной оплаты.</p>
                    </div>
                    <div className="bg-[var(--bg-sheet)] rounded p-3 hover:bg-[var(--bg-elevated)] transition-colors">
                        <h3 className="font-bold text-sm text-[var(--fg1)] mb-1">Для любой встречи</h3>
                        <p className="text-xs text-[var(--fg2)] leading-relaxed">Google Meet, Zoom, Teams и разговоры офлайн.</p>
                    </div>
                </div>
            </div>

            {/* Coming Soon - Compact */}
            <div className="bg-[var(--gold-soft)] rounded p-3">
                <p className="text-s text-[var(--gold)]">
                    <span className="font-bold">Дальше:</span> локальные помощники для задач и договорённостей после встречи.
                </p>
            </div>

            {/* CTA Section - Compact */}
            <div className="text-center space-y-2">
                <h3 className="text-medium font-semibold text-[var(--fg1)]">Нужен Memento для команды</h3>
                <p className="text-s text-[var(--fg2)]">
                    Можно обсудить частное развёртывание и адаптацию продукта под процессы команды.
                </p>
                <button
                    onClick={handleContactClick}
                    className="inline-flex items-center px-4 py-2 bg-[var(--gold)] hover:bg-[var(--gold-active)] text-[var(--fg-inverse)] text-sm font-medium rounded transition-colors duration-200 shadow-none hover:shadow-none"
                >
                    Связаться с командой
                </button>
            </div>

            {/* Footer - Compact */}
            <div className="pt-2 border-t border-[var(--border-subtle)] text-center">
                <p className="text-xs text-[var(--fg3)]">
                    Сделано Zackriya Solutions
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
