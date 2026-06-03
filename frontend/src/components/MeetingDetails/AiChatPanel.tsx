"use client";

import { useState, useRef, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { invoke } from '@tauri-apps/api/core';
import { Transcript } from '@/types';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { Loader2, Send, User, Sparkles, RefreshCcw } from 'lucide-react';
import { toast } from 'sonner';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface AiChatPanelProps {
  meetingId: string;
  transcripts: Transcript[];
  modelConfig?: ModelConfig;
}

export function AiChatPanel({ meetingId, transcripts, modelConfig }: AiChatPanelProps) {
  const [messages, setMessages] = useState<{role: string, content: string}[]>([]);
  const [input, setInput] = useState('');
  const [isGenerating, setIsGenerating] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Fetch history on mount
  useEffect(() => {
    const fetchHistory = async () => {
      try {
        const history = await invoke<{role: string, content: string}[]>('api_get_chat_messages', { meetingId });
        if (history && history.length > 0) {
          setMessages(history);
        }
      } catch (e) {
        console.error("Failed to fetch chat history", e);
      }
    };
    fetchHistory();
  }, [meetingId]);

  // Auto-scroll to bottom
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, isGenerating]);

  const handleSend = async () => {
    if (!input.trim() || isGenerating) return;
    if (!modelConfig) {
      toast.error("Model configuration is missing.");
      return;
    }

    const userMsg = input.trim();
    setInput('');
    const newMessages = [...messages, { role: 'user', content: userMsg }];
    setMessages(newMessages);
    setIsGenerating(true);

    try {
      // Save user message
      await invoke('api_save_chat_message', { meetingId, role: 'user', content: userMsg }).catch(e => console.error("Failed to save user message", e));

      const systemPrompt = `You are a helpful AI assistant. Use the following meeting transcript to answer the user's questions or requests.\n\nTRANSCRIPT:\n${transcripts.map(t => t.text).join('\n')}`;

      const response = await invoke<string>('api_chat_with_transcript', {
        meetingId,
        systemPrompt,
        messages: newMessages,
        modelProvider: modelConfig.provider,
        modelName: modelConfig.model,
      });

      // Save AI message
      await invoke('api_save_chat_message', { meetingId, role: 'assistant', content: response }).catch(e => console.error("Failed to save assistant message", e));

      setMessages(prev => [...prev, { role: 'assistant', content: response }]);
    } catch (e: any) {
      console.error("Chat error", e);
      toast.error(e.toString());
    } finally {
      setIsGenerating(false);
    }
  };

  const handleRegenerate = async () => {
    if (isGenerating || messages.length === 0) return;
    if (!modelConfig) {
      toast.error("Model configuration is missing.");
      return;
    }
    
    // Find the last user message
    const lastUserMsgIdx = [...messages].reverse().findIndex(m => m.role === 'user');
    if (lastUserMsgIdx === -1) return;
    
    const actualIdx = messages.length - 1 - lastUserMsgIdx;
    
    // Keep messages up to the last user message
    const newMessages = messages.slice(0, actualIdx + 1);
    setMessages(newMessages);
    setIsGenerating(true);

    try {
      const systemPrompt = `You are a helpful AI assistant. Use the following meeting transcript to answer the user's questions or requests.\n\nTRANSCRIPT:\n${transcripts.map(t => t.text).join('\n')}`;

      const response = await invoke<string>('api_chat_with_transcript', {
        meetingId,
        systemPrompt,
        messages: newMessages,
        modelProvider: modelConfig.provider,
        modelName: modelConfig.model,
      });

      // Save AI message
      await invoke('api_save_chat_message', { meetingId, role: 'assistant', content: response }).catch(e => console.error("Failed to save assistant message", e));

      setMessages([...newMessages, { role: 'assistant', content: response }]);
    } catch (e: any) {
      console.error("Chat error", e);
      toast.error(e.toString());
    } finally {
      setIsGenerating(false);
    }
  };

  return (
    <div className="flex flex-col h-full bg-white relative">
      <div className="flex-1 p-4 overflow-y-auto" ref={scrollRef}>
        <div className="space-y-4 pb-4">
          {messages.length === 0 ? (
            <div className="text-center text-sm text-gray-500 mt-10">
              Send a message to start chatting!
            </div>
          ) : (
            messages.map((msg, idx) => (
              <div key={idx} className={`flex gap-3 items-start ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                {msg.role === 'assistant' && (
                  <div className="w-8 h-8 rounded-full bg-gradient-to-tr from-indigo-500 to-purple-500 flex items-center justify-center shrink-0 shadow-sm mt-1">
                    <Sparkles className="w-4 h-4 text-white" />
                  </div>
                )}
                <div className={`px-4 py-3 rounded-2xl max-w-[85%] text-sm ${
                  msg.role === 'user' ? 'bg-blue-600 text-white rounded-tr-sm' : 'bg-gray-100 text-gray-800 rounded-tl-sm'
                }`}>
                  {msg.role === 'user' ? (
                    <div className="whitespace-pre-wrap">{msg.content}</div>
                  ) : (
                    <ReactMarkdown remarkPlugins={[remarkGfm]} className="prose prose-sm max-w-none prose-p:leading-relaxed prose-pre:bg-gray-800 prose-pre:text-gray-100 break-words prose-th:p-2 prose-td:p-2 prose-table:border prose-th:border prose-td:border">
                      {msg.content}
                    </ReactMarkdown>
                  )}
                </div>
                {msg.role === 'user' && (
                  <div className="w-8 h-8 rounded-full bg-gray-200 flex items-center justify-center shrink-0">
                    <User className="w-5 h-5 text-gray-600" />
                  </div>
                )}
              </div>
            ))
          )}
          {!isGenerating && messages.length > 0 && (
            <div className={`flex ${messages[messages.length - 1].role === 'user' ? 'justify-end pr-11' : 'justify-start pl-11'}`}>
              <Button variant="ghost" size="sm" onClick={handleRegenerate} className="text-xs text-gray-500 hover:text-gray-700 h-7 px-2">
                <RefreshCcw className="w-3 h-3 mr-1.5" />
                {messages[messages.length - 1].role === 'user' ? 'Resend Message' : 'Regenerate Response'}
              </Button>
            </div>
          )}
          {isGenerating && (
            <div className="flex gap-3 items-start justify-start">
              <div className="w-8 h-8 rounded-full bg-gradient-to-tr from-indigo-500 to-purple-500 flex items-center justify-center shrink-0 shadow-sm mt-1">
                <Sparkles className="w-4 h-4 text-white" />
              </div>
              <div className="px-4 py-3 rounded-2xl rounded-tl-sm bg-gray-100 text-gray-800 flex items-center min-h-[44px]">
                <Loader2 className="w-4 h-4 animate-spin text-gray-500" />
              </div>
            </div>
          )}
        </div>
      </div>

      <div className="p-3 border-t flex gap-2 shrink-0 bg-gray-50">
        <Textarea 
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              handleSend();
            }
          }}
          placeholder="Ask a question..."
          className="min-h-[2.5rem] max-h-32 resize-none flex-1 py-2"
          rows={1}
        />
        <Button size="icon" disabled={!input.trim() || isGenerating} onClick={handleSend} className="shrink-0 h-[2.5rem] w-[2.5rem]">
          <Send className="w-4 h-4" />
        </Button>
      </div>
    </div>
  );
}
