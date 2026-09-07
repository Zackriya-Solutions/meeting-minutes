'use client';

import React, { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { useRouter } from 'next/navigation';
import {
  Bot,
  X,
  RotateCcw,
  Send,
  Copy,
  Check,
  Layers,
  ArrowRight,
  Clock,
  CheckSquare,
  ListTodo,
  FileText
} from 'lucide-react';
import dynamic from 'next/dynamic';
import { toast } from 'sonner';

import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
  modelInfo?: string;
}

export interface MeetingItem {
  id: string;
  title: string;
}

interface AskMeetingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  meetings: MeetingItem[];
  currentMeetingId?: string | null;
  onOpenSettings?: () => void;
}

// Robust message renderer with GFM support
function SafeMessageRenderer({ content }: { content: string }) {
  return (
    <div className="prose prose-sm max-w-none text-gray-800 leading-relaxed break-words">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>
        {content}
      </ReactMarkdown>
    </div>
  );
}

export function AskMeetingsModal({
  isOpen,
  onClose,
  meetings = [],
  currentMeetingId,
  onOpenSettings
}: AskMeetingsModalProps) {
  const router = useRouter();
  const [mounted, setMounted] = useState(false);
  const [selectedScope, setSelectedScope] = useState<string>('all');
  const [meetingList, setMeetingList] = useState<MeetingItem[]>(meetings);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [modelConfig, setModelConfig] = useState<{ provider: string; model: string }>({
    provider: 'Built-in AI',
    model: 'Qwen 3.5 4B'
  });

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Track client mount for createPortal
  useEffect(() => {
    setMounted(true);
  }, []);

  // Sync meetings prop to local state
  useEffect(() => {
    if (meetings && meetings.length > 0) {
      setMeetingList(meetings);
    }
  }, [meetings]);

  // Fetch fresh meetings list directly from backend on open
  useEffect(() => {
    if (!isOpen) return;

    const fetchMeetingsDirect = async () => {
      try {
        const data = (await invoke('api_get_meetings')) as Array<{ id: string; title: string }>;
        if (data && Array.isArray(data)) {
          setMeetingList(data);
        }
      } catch (err) {
        console.warn('Could not refresh meetings list:', err);
      }
    };

    fetchMeetingsDirect();
  }, [isOpen]);

  // Initialize scope when modal opens or currentMeetingId changes
  useEffect(() => {
    if (isOpen) {
      if (currentMeetingId && meetingList.some(m => m.id === currentMeetingId)) {
        setSelectedScope(currentMeetingId);
      } else if (meetingList.length > 0 && selectedScope !== 'all' && !meetingList.some(m => m.id === selectedScope)) {
        setSelectedScope('all');
      }
    }
  }, [isOpen, currentMeetingId, meetingList]);

  // Fetch active global AI model configuration & listen for changes
  useEffect(() => {
    const fetchModelInfo = async () => {
      try {
        const data = (await invoke('api_get_model_config')) as any;
        if (data && data.provider) {
          let providerDisplay = data.provider;
          if (data.provider === 'openrouter') providerDisplay = 'OpenRouter';
          else if (data.provider === 'openai') providerDisplay = 'OpenAI';
          else if (data.provider === 'claude') providerDisplay = 'Claude';
          else if (data.provider === 'groq') providerDisplay = 'Groq';
          else if (data.provider === 'ollama') providerDisplay = 'Ollama';
          else if (data.provider === 'builtin-ai') providerDisplay = 'Built-in AI';
          else if (data.provider === 'custom-openai') providerDisplay = 'Custom OpenAI';

          setModelConfig({
            provider: providerDisplay,
            model: data.model || 'Default'
          });
        }
      } catch (err) {
        console.warn('Could not fetch model config for Ask AI modal:', err);
      }
    };

    if (isOpen) {
      fetchModelInfo();
    }

    // Listen for live model config updates
    const setupListener = async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        const unlisten = await listen<any>('model-config-updated', (event) => {
          if (event.payload && event.payload.provider) {
            let providerDisplay = event.payload.provider;
            if (providerDisplay === 'openrouter') providerDisplay = 'OpenRouter';
            else if (providerDisplay === 'openai') providerDisplay = 'OpenAI';
            else if (providerDisplay === 'claude') providerDisplay = 'Claude';
            else if (providerDisplay === 'groq') providerDisplay = 'Groq';
            else if (providerDisplay === 'ollama') providerDisplay = 'Ollama';
            else if (providerDisplay === 'builtin-ai') providerDisplay = 'Built-in AI';
            else if (providerDisplay === 'custom-openai') providerDisplay = 'Custom OpenAI';

            setModelConfig({
              provider: providerDisplay,
              model: event.payload.model || 'Default'
            });
          }
        });
        return unlisten;
      } catch (e) {
        console.warn('Failed to listen for model-config-updated in modal:', e);
      }
    };

    const unlistenPromise = setupListener();
    return () => {
      unlistenPromise.then(fn => fn && fn());
    };
  }, [isOpen]);

  // Focus input when modal opens
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => {
        inputRef.current?.focus();
      }, 120);
    }
  }, [isOpen]);

  // Auto scroll to bottom on new messages or loading
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, loading]);

  if (!isOpen || !mounted) return null;

  const formatCurrentTime = () => {
    const now = new Date();
    return now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
  };

  const handleSend = async (queryText?: string) => {
    const textToSend = (queryText || input).trim();
    if (!textToSend || loading) return;

    const userTime = formatCurrentTime();
    const userMessage: ChatMessage = {
      id: `user-${Date.now()}`,
      role: 'user',
      content: textToSend,
      timestamp: userTime
    };

    const newMessages = [...messages, userMessage];
    setMessages(newMessages);
    setInput('');
    setLoading(true);

    try {
      const historyForBackend = newMessages.map(m => ({
        role: m.role,
        content: m.content
      }));

      const res = (await invoke('api_ask_meetings_ai', {
        query: textToSend,
        meetingId: selectedScope === 'all' ? null : selectedScope,
        history: historyForBackend
      })) as { answer: string; provider: string; model: string };

      let providerDisplay = res.provider;
      if (res.provider.toLowerCase() === 'openrouter') providerDisplay = 'OpenRouter';
      else if (res.provider.toLowerCase() === 'openai') providerDisplay = 'OpenAI';
      else if (res.provider.toLowerCase() === 'claude') providerDisplay = 'Claude';
      else if (res.provider.toLowerCase() === 'groq') providerDisplay = 'Groq';
      else if (res.provider.toLowerCase() === 'ollama') providerDisplay = 'Ollama';
      else if (res.provider.toLowerCase() === 'builtin-ai') providerDisplay = 'Built-in AI';
      else if (res.provider.toLowerCase() === 'custom-openai') providerDisplay = 'Custom OpenAI';

      const assistantTime = formatCurrentTime();
      const assistantMessage: ChatMessage = {
        id: `assistant-${Date.now()}`,
        role: 'assistant',
        content: res.answer,
        timestamp: assistantTime,
        modelInfo: `${providerDisplay} • ${res.model}`
      };

      setMessages(prev => [...prev, assistantMessage]);
    } catch (error: any) {
      console.error('Ask AI error:', error);
      const errStr = typeof error === 'string' ? error : error?.message || 'Failed to generate response';
      const assistantMessage: ChatMessage = {
        id: `assistant-${Date.now()}`,
        role: 'assistant',
        content: `⚠️ **Error**\n\n${errStr}\n\n*If using an online provider, verify your API key in Settings. If using Built-in AI or Ollama, ensure the model is downloaded.*`,
        timestamp: formatCurrentTime(),
        modelInfo: `${modelConfig.provider} • ${modelConfig.model}`
      };
      setMessages(prev => [...prev, assistantMessage]);
      toast.error('AI response failed', { description: errStr });
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    toast.success('Copied to clipboard');
    setTimeout(() => setCopiedId(null), 2000);
  };

  const handleReset = () => {
    setMessages([]);
    setInput('');
  };

  const handleSettingsClick = () => {
    onClose();
    if (onOpenSettings) {
      onOpenSettings();
    } else {
      router.push('/settings?tab=summary');
    }
  };

  const selectedMeeting = meetingList.find(m => m.id === selectedScope);
  const isAllScope = selectedScope === 'all';

  const modalContent = (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-in fade-in duration-200"
      onClick={onClose}
    >
      <div
        className="relative w-full max-w-2xl sm:max-w-3xl bg-white rounded-2xl shadow-2xl border border-gray-200 flex flex-col max-h-[92vh] overflow-hidden"
        onClick={e => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-100 bg-white">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-xl bg-blue-600 flex items-center justify-center text-white shadow-sm flex-shrink-0">
              <Bot className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-lg font-bold text-gray-900">Ask Your Meetings AI</h2>
            </div>
          </div>

          <div className="flex items-center gap-1">
            <button
              onClick={handleReset}
              className="p-2 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-full transition-colors"
              title="Reset conversation"
            >
              <RotateCcw className="w-4 h-4" />
            </button>
            <button
              onClick={onClose}
              className="p-2 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-full transition-colors"
              title="Close"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* Scope Selector Bar */}
        <div className="px-6 py-2.5 bg-gray-50/90 border-b border-gray-100 flex items-center gap-3 text-xs text-gray-600">
          <div className="flex items-center gap-1.5 font-medium text-gray-700 flex-shrink-0">
            <Layers className="w-3.5 h-3.5 text-gray-500" />
            <span>Scope:</span>
          </div>

          <div className="relative flex-1 max-w-md">
            <select
              value={selectedScope}
              onChange={e => setSelectedScope(e.target.value)}
              className="w-full pl-3 pr-8 py-1.5 text-xs font-medium text-gray-800 bg-white border border-gray-200 rounded-lg shadow-sm hover:border-gray-300 focus:outline-none focus:ring-1 focus:ring-blue-500 appearance-none cursor-pointer truncate"
            >
              <option value="all">🌐 All Meeting Notes ({meetingList.length})</option>
              {meetingList.map(meeting => (
                <option key={meeting.id} value={meeting.id}>
                  📌 {meeting.title}
                </option>
              ))}
            </select>
            <div className="absolute inset-y-0 right-0 flex items-center px-2 pointer-events-none text-gray-400">
              <svg className="w-3 h-3 fill-current" viewBox="0 0 20 20">
                <path d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z" />
              </svg>
            </div>
          </div>
        </div>

        {/* Chat Messages Container */}
        <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4 min-h-[320px] max-h-[480px] bg-slate-50/30">
          {messages.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full py-8 text-center text-gray-500">
              <div className="w-12 h-12 rounded-2xl bg-blue-50 text-blue-600 flex items-center justify-center mb-3">
                <Bot className="w-6 h-6" />
              </div>
              <h3 className="text-sm font-semibold text-gray-800 mb-1">
                {isAllScope
                  ? 'Ask anything across all your meeting notes'
                  : `Ask anything about "${selectedMeeting?.title || 'this meeting'}"`}
              </h3>
              <p className="text-xs text-gray-500 max-w-md mb-5">
                Instant conversational intelligence on meeting duration, key decisions, action items, and topic discussions.
              </p>

              {/* Quick suggestions */}
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 w-full max-w-lg text-left">
                <button
                  onClick={() => handleSend('time taken by this meeting')}
                  className="p-2.5 text-xs bg-white border border-gray-200 hover:border-blue-300 hover:bg-blue-50/50 rounded-xl transition-all flex items-center gap-2 text-gray-700 shadow-sm"
                >
                  <Clock className="w-3.5 h-3.5 text-blue-600 flex-shrink-0" />
                  <span className="truncate">time taken by this meeting</span>
                </button>
                <button
                  onClick={() => handleSend('Summarize the key decisions made in this meeting')}
                  className="p-2.5 text-xs bg-white border border-gray-200 hover:border-blue-300 hover:bg-blue-50/50 rounded-xl transition-all flex items-center gap-2 text-gray-700 shadow-sm"
                >
                  <CheckSquare className="w-3.5 h-3.5 text-emerald-600 flex-shrink-0" />
                  <span className="truncate">Summarize key decisions made</span>
                </button>
                <button
                  onClick={() => handleSend('What are the action items and deliverables?')}
                  className="p-2.5 text-xs bg-white border border-gray-200 hover:border-blue-300 hover:bg-blue-50/50 rounded-xl transition-all flex items-center gap-2 text-gray-700 shadow-sm"
                >
                  <ListTodo className="w-3.5 h-3.5 text-blue-600 flex-shrink-0" />
                  <span className="truncate">What are the action items?</span>
                </button>
                <button
                  onClick={() => handleSend('Give a quick high-level summary of the discussions')}
                  className="p-2.5 text-xs bg-white border border-gray-200 hover:border-blue-300 hover:bg-blue-50/50 rounded-xl transition-all flex items-center gap-2 text-gray-700 shadow-sm"
                >
                  <FileText className="w-3.5 h-3.5 text-blue-600 flex-shrink-0" />
                  <span className="truncate">Give a high-level summary</span>
                </button>
              </div>
            </div>
          ) : (
            messages.map(msg => (
              <div key={msg.id}>
                {msg.role === 'user' ? (
                  /* User Bubble */
                  <div className="flex flex-col items-end">
                    <div className="bg-blue-600 text-white rounded-2xl rounded-tr-sm px-4 py-2.5 text-sm max-w-[85%] shadow-sm leading-relaxed">
                      {msg.content}
                    </div>
                    <span className="text-[10px] text-gray-400 mt-1 mr-1">{msg.timestamp}</span>
                  </div>
                ) : (
                  /* AI Assistant Bubble */
                  <div className="flex items-start gap-3 max-w-[95%]">
                    <div className="w-8 h-8 rounded-full bg-blue-600 text-white flex items-center justify-center flex-shrink-0 mt-0.5 shadow-sm">
                      <Bot className="w-4 h-4" />
                    </div>

                    <div className="flex-1 bg-white border border-gray-100 shadow-sm rounded-2xl p-4 text-sm text-gray-800">
                      <SafeMessageRenderer content={msg.content} />

                      {/* Message Footer */}
                      <div className="flex items-center justify-between mt-3 pt-2.5 border-t border-gray-100 text-[11px] text-gray-400">
                        <div className="flex items-center gap-2">
                          <span>{msg.timestamp}</span>
                          {msg.modelInfo && (
                            <span className="inline-flex items-center gap-1 bg-gray-100/90 text-gray-600 text-[10px] font-medium px-2 py-0.5 rounded-full border border-gray-200/60">
                              ⚡ {msg.modelInfo}
                            </span>
                          )}
                        </div>

                        <button
                          onClick={() => handleCopy(msg.content, msg.id)}
                          className="flex items-center gap-1 text-gray-500 hover:text-gray-800 px-2 py-1 hover:bg-gray-100 rounded transition-colors"
                          title="Copy response"
                        >
                          {copiedId === msg.id ? (
                            <>
                              <Check className="w-3.5 h-3.5 text-emerald-600" />
                              <span className="text-emerald-600 text-xs">Copied</span>
                            </>
                          ) : (
                            <>
                              <Copy className="w-3.5 h-3.5" />
                              <span className="text-xs">Copy</span>
                            </>
                          )}
                        </button>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            ))
          )}

          {/* Loading Indicator */}
          {loading && (
            <div className="flex items-start gap-3">
              <div className="w-7 h-7 rounded-full bg-blue-600 text-white flex items-center justify-center flex-shrink-0 mt-1 shadow-sm animate-pulse">
                <Bot className="w-3.5 h-3.5" />
              </div>
              <div className="bg-white border border-gray-100 rounded-2xl px-4 py-3 shadow-sm flex items-center gap-2 text-xs text-gray-500">
                <div className="flex gap-1">
                  <span className="w-1.5 h-1.5 rounded-full bg-blue-600 animate-bounce [animation-delay:-0.3s]"></span>
                  <span className="w-1.5 h-1.5 rounded-full bg-blue-600 animate-bounce [animation-delay:-0.15s]"></span>
                  <span className="w-1.5 h-1.5 rounded-full bg-blue-600 animate-bounce"></span>
                </div>
                <span>Analyzing meeting context and generating answer...</span>
              </div>
            </div>
          )}

          <div ref={messagesEndRef} />
        </div>

        {/* Bottom AI Status & Settings Bar */}
        <div className="px-6 py-2.5 bg-gray-50 border-t border-gray-100 flex items-center justify-between text-xs text-gray-500">
          <div className="flex items-center gap-2">
            <span className="font-medium text-gray-700">
              AI Model: {modelConfig.provider} • {modelConfig.model}
            </span>
            <span className="inline-flex items-center gap-1 text-[11px] font-medium text-emerald-600 bg-emerald-50 px-2 py-0.5 rounded-full border border-emerald-200">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
              {loading ? 'Generating...' : 'Ready'}
            </span>
          </div>

          <button
            onClick={handleSettingsClick}
            className="text-blue-600 hover:text-blue-800 font-medium flex items-center gap-1 transition-colors"
          >
            <span>Configure in Settings</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        </div>

        {/* Input Bar */}
        <div className="p-4 bg-white border-t border-gray-100">
          <form
            onSubmit={e => {
              e.preventDefault();
              handleSend();
            }}
            className="flex items-center gap-2"
          >
            <input
              ref={inputRef}
              type="text"
              value={input}
              onChange={e => setInput(e.target.value)}
              placeholder={
                isAllScope
                  ? 'Ask anything across all your meeting notes...'
                  : 'Ask anything about this specific meeting note...'
              }
              disabled={loading}
              className="flex-1 px-4 py-2.5 text-sm bg-gray-50 border border-gray-200 rounded-xl focus:outline-none focus:ring-2 focus:ring-blue-500/20 focus:border-blue-500 text-gray-900 placeholder:text-gray-400 transition-all disabled:opacity-50"
            />
            <button
              type="submit"
              disabled={!input.trim() || loading}
              className="px-4 py-2.5 bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-xl font-medium text-sm flex items-center gap-1.5 shadow-sm transition-all flex-shrink-0"
            >
              <Send className="w-4 h-4" />
              <span>Ask AI</span>
            </button>
          </form>
        </div>
      </div>
    </div>
  );

  return createPortal(modalContent, document.body);
}
