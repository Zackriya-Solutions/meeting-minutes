import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Plus, Trash2, Save, FlaskConical } from 'lucide-react';

interface SubstitutionRule {
    find: string;
    replace: string;
    wholeWord: boolean;
    caseSensitive: boolean;
    enabled: boolean;
}

export function SubstitutionsSettings() {
    const [rules, setRules] = useState<SubstitutionRule[]>([]);
    const [filePath, setFilePath] = useState<string>('');
    const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle');
    const [sampleText, setSampleText] = useState('We drove down to tuttle to see the newplains office.');
    const [testResult, setTestResult] = useState<string>('');

    useEffect(() => {
        invoke<SubstitutionRule[]>('substitutions_list')
            .then((r) => setRules(r ?? []))
            .catch((e) => console.error('Failed to load substitutions:', e));
        invoke<string>('substitutions_file_path')
            .then(setFilePath)
            .catch((e) => console.error('Failed to get substitutions path:', e));
    }, []);

    const updateRule = (index: number, patch: Partial<SubstitutionRule>) => {
        setRules((prev) => prev.map((r, i) => (i === index ? { ...r, ...patch } : r)));
        setSaveState('idle');
    };

    const addRule = () => {
        setRules((prev) => [
            ...prev,
            { find: '', replace: '', wholeWord: true, caseSensitive: false, enabled: true },
        ]);
        setSaveState('idle');
    };

    const removeRule = (index: number) => {
        setRules((prev) => prev.filter((_, i) => i !== index));
        setSaveState('idle');
    };

    const save = async () => {
        setSaveState('saving');
        try {
            await invoke('substitutions_save', { rules });
            setSaveState('saved');
        } catch (e) {
            console.error('Failed to save substitutions:', e);
            setSaveState('error');
        }
    };

    const runTest = async () => {
        try {
            const result = await invoke<string>('substitutions_test', { text: sampleText });
            setTestResult(result);
        } catch (e) {
            setTestResult(`Test failed: ${e}`);
        }
    };

    return (
        <div className="space-y-6">
            <div>
                <h2 className="text-lg font-semibold">Vocabulary & Spelling</h2>
                <p className="text-sm text-muted-foreground">
                    Words to correct in every transcript. Rules apply to new transcriptions, live and
                    imported, and the glossary is fed to the speech engine so names are recognized
                    correctly in the first place.
                </p>
                {filePath && (
                    <p className="text-xs text-muted-foreground mt-1">File: <code>{filePath}</code></p>
                )}
            </div>

            <div className="space-y-2">
                <div className="grid grid-cols-[1fr_1fr_auto_auto_auto_auto] gap-2 text-xs text-muted-foreground px-1">
                    <span>Heard as</span>
                    <span>Replace with</span>
                    <span>Whole word</span>
                    <span>Case sens.</span>
                    <span>On</span>
                    <span />
                </div>
                {rules.length === 0 && (
                    <p className="text-sm text-muted-foreground px-1 py-4">
                        No rules yet — add ones for names, companies, and jargon Whisper mishears.
                    </p>
                )}
                {rules.map((rule, i) => (
                    <div key={i} className="grid grid-cols-[1fr_1fr_auto_auto_auto_auto] gap-2 items-center">
                        <Input
                            value={rule.find}
                            placeholder="e.g. skags"
                            onChange={(e) => updateRule(i, { find: e.target.value })}
                            className="h-8"
                        />
                        <Input
                            value={rule.replace}
                            placeholder="e.g. Skaggs"
                            onChange={(e) => updateRule(i, { replace: e.target.value })}
                            className="h-8"
                        />
                        <input
                            type="checkbox"
                            checked={rule.wholeWord}
                            onChange={(e) => updateRule(i, { wholeWord: e.target.checked })}
                            className="mx-auto"
                            title="Match whole words only"
                        />
                        <input
                            type="checkbox"
                            checked={rule.caseSensitive}
                            onChange={(e) => updateRule(i, { caseSensitive: e.target.checked })}
                            className="mx-auto"
                            title="Case-sensitive match"
                        />
                        <input
                            type="checkbox"
                            checked={rule.enabled}
                            onChange={(e) => updateRule(i, { enabled: e.target.checked })}
                            className="mx-auto"
                            title="Rule enabled"
                        />
                        <Button variant="ghost" size="sm" onClick={() => removeRule(i)} title="Delete rule">
                            <Trash2 className="h-4 w-4" />
                        </Button>
                    </div>
                ))}
                <div className="flex gap-2 pt-2">
                    <Button variant="outline" size="sm" onClick={addRule}>
                        <Plus className="h-4 w-4 mr-1" /> Add Rule
                    </Button>
                    <Button size="sm" onClick={save} disabled={saveState === 'saving'}>
                        <Save className="h-4 w-4 mr-1" />
                        {saveState === 'saving' ? 'Saving…' : saveState === 'saved' ? 'Saved' : 'Save'}
                    </Button>
                    {saveState === 'saved' && <span className="text-sm text-green-600 self-center">Applied to next transcription — no restart needed.</span>}
                    {saveState === 'error' && <span className="text-sm text-red-600 self-center">Save failed — check the log.</span>}
                </div>
            </div>

            <div className="space-y-2 border-t pt-4">
                <Label>Test area</Label>
                <Input
                    value={sampleText}
                    onChange={(e) => setSampleText(e.target.value)}
                    placeholder="Paste sample transcript text"
                />
                <Button variant="outline" size="sm" onClick={runTest}>
                    <FlaskConical className="h-4 w-4 mr-1" /> Test
                </Button>
                {testResult && (
                    <div className="rounded-md bg-muted p-3 text-sm whitespace-pre-wrap">{testResult}</div>
                )}
            </div>
        </div>
    );
}
