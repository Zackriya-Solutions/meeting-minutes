import React from 'react';

interface AlertDialogProps {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  children: React.ReactNode;
}

export function AlertDialog({ open, onOpenChange, children }: AlertDialogProps) {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 bg-black/50 flex items-center justify-center">
      <div className="bg-white rounded-lg p-6 max-w-md w-full mx-4">
        {children}
      </div>
    </div>
  );
}

export function AlertDialogContent({ children }: { children: React.ReactNode }) {
  return <div>{children}</div>;
}

export function AlertDialogHeader({ children }: { children: React.ReactNode }) {
  return <div className="mb-4">{children}</div>;
}

export function AlertDialogFooter({ children }: { children: React.ReactNode }) {
  return <div className="flex justify-end space-x-2 mt-4">{children}</div>;
}

export function AlertDialogTitle({ children }: { children: React.ReactNode }) {
  return <h2 className="text-xl font-semibold">{children}</h2>;
}

export function AlertDialogDescription({ children }: { children: React.ReactNode }) {
  return <p className="text-gray-500 mt-2">{children}</p>;
}

export function AlertDialogAction({ children, onClick }: { children: React.ReactNode; onClick?: () => void }) {
  return (
    <button
      onClick={onClick}
      className="bg-red-500 text-white px-4 py-2 rounded hover:bg-red-600 transition-colors"
    >
      {children}
    </button>
  );
}

export function AlertDialogCancel({ children }: { children: React.ReactNode }) {
  return (
    <button className="bg-gray-100 text-gray-700 px-4 py-2 rounded hover:bg-gray-200 transition-colors">
      {children}
    </button>
  );
} 