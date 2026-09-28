import { Search, X } from "lucide-react";
import { forwardRef } from "react";
import { useStore } from "../../state/store";

export const SearchField = forwardRef<HTMLInputElement>(function SearchField(_props, ref) {
  const search = useStore((s) => s.search);
  const setSearch = useStore((s) => s.setSearch);
  return (
    <label className="flex h-9 items-center gap-2 rounded-control border border-line bg-raised px-3 focus-within:border-accent">
      <Search size={15} strokeWidth={1.75} className="shrink-0 text-muted" aria-hidden="true" />
      <input
        ref={ref}
        type="search"
        value={search}
        onChange={(e) => {
          setSearch(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape" && search) {
            setSearch("");
            e.stopPropagation();
          }
        }}
        placeholder="Search storage…"
        aria-label="Search storage"
        spellCheck={false}
        className="min-w-0 flex-1 bg-transparent text-text outline-none placeholder:text-muted"
      />
      {search && (
        <button
          type="button"
          aria-label="Clear search"
          onClick={() => {
            setSearch("");
          }}
          className="text-muted hover:text-text"
        >
          <X size={14} aria-hidden="true" />
        </button>
      )}
    </label>
  );
});
