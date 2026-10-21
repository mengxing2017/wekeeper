import { useState, type FormEvent } from 'react';
import { Alert } from 'antd';

interface Field {
    name: string;
    title: string;
    type?: string;
    options?: {
        value: string;
        label: string;
    }[];
    optional?: boolean;
    readOnly?: boolean;
    value?: string | number;
}
export function Editor({ fields, onSave, submit = '保存' }: {
    fields: Field[];
    onSave: (v: Record<string, string>) => Promise<unknown>;
    submit?: string;
}) {
    const [busy, setBusy] = useState(false), [error, setError] = useState(''), [done, setDone] = useState(false);
    async function save(e: FormEvent<HTMLFormElement>) { e.preventDefault(); const form = e.currentTarget; const v = Object.fromEntries(new FormData(form).entries()) as Record<string, string>; setBusy(true); setError(''); setDone(false); try {
        await onSave(v);
        setDone(true);
    }
    catch (e) {
        setError((e as Error).message);
    }
    finally {
        setBusy(false);
    } }
    return <form onSubmit={save} className="business-form">{fields.map(f => <label key={f.name}>{f.title}{f.type === 'textarea' ? <textarea name={f.name} required={!f.optional} defaultValue={f.value} rows={4}/> : f.options ? <select name={f.name} required={!f.optional} defaultValue={f.value}><option value="">请选择</option>{f.options.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}</select> : <input readOnly={f.readOnly} name={f.name} type={f.type ?? 'text'} required={!f.optional} defaultValue={f.value}/>}</label>)}{error && <Alert type="error" title={error}/>}{done && <p role="status">已保存</p>}<button disabled={busy}>{busy ? '正在保存…' : submit}</button></form>;
}
