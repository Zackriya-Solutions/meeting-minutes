import {useEffect, useState} from 'react';

interface MessageToastProps {
    message: string;
    type: 'success' | 'error';
    show: boolean;
    setShow: (show: boolean) => void;
}

export function MessageToast({ message, type, show, setShow }: MessageToastProps) {
    
    useEffect(() => {
        const timer = setTimeout(() => {
            setShow(false);
        }, 3000);
        
        return () => clearTimeout(timer);
    }, []); 
    
    return (
        show && (
            <span className={`${type === 'success' ? 'text-success' : 'text-destructive'}`}>{message}</span>
        )
    );
}