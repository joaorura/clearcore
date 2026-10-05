import { useState } from 'react';

/** Short success banner above the tabs (sample captured, take approved, profile built...). */
export function useFlashMessage(): { message: string | null; flash: (message: string, ms: number) => void } {
  const [message, setMessage] = useState<string | null>(null);
  const flash = (text: string, ms: number) => {
    setMessage(text);
    setTimeout(() => setMessage(null), ms);
  };
  return { message, flash };
}
