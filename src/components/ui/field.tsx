import type { ReactNode } from "react";

export interface FieldProps {
  label: string;
  id?: string;
  children: ReactNode;
}

/** Label + control wrapper shared by central forms. */
export function Field({ label, id, children }: FieldProps) {
  if (id) {
    return (
      <div className="block space-y-1.5 text-[13px]">
        <label htmlFor={id} className="font-medium">
          {label}
        </label>
        {children}
      </div>
    );
  }

  return (
    <label className="block space-y-1.5 text-[13px]">
      <span className="font-medium">{label}</span>
      {children}
    </label>
  );
}
