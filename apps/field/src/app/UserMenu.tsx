import { Popover as PopoverPrimitive } from '@base-ui/react/popover'
import { User } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { LanguageSwitch } from '@/app/LanguageSwitch'
import { SignOutButton } from '@/app/SignOutButton'
import { useSession } from '@/app/session'
import { TourButton } from '@/app/tour/TourButton'

/**
 * The mobile header's collapsed actions — spec 4.1's name pill, tour restart,
 * language switch and sign out, folded behind one trigger below `sm` instead
 * of rendered as separate full-width buttons. The `sm:` and up header keeps
 * that row exactly as it was; this only ever mounts in the mobile row.
 */
export function UserMenu() {
  const { t } = useTranslation()
  const { data: session } = useSession()

  return (
    <PopoverPrimitive.Root>
      <PopoverPrimitive.Trigger
        data-testid="user-menu-trigger"
        aria-label={t('nav.userMenu')}
        className="inline-flex min-h-11 min-w-11 items-center justify-center rounded-full border-[1.5px] border-rule-2 bg-paper text-ink-2 outline-none focus-visible:ring-2 focus-visible:ring-primary"
      >
        <User aria-hidden size={20} strokeWidth={2} />
      </PopoverPrimitive.Trigger>
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Positioner side="bottom" align="end" sideOffset={8} className="z-40">
          <PopoverPrimitive.Popup
            data-testid="user-menu-popup"
            className="flex w-64 flex-col gap-3 rounded-[var(--radius-control)] border border-rule-2 bg-paper p-3 text-sm text-ink"
          >
            {session?.appUser && (
              <span data-testid="user-menu-name" className="font-medium text-ink">
                {session.appUser.display_name}
              </span>
            )}
            <LanguageSwitch className="w-full" />
            <TourButton />
            <SignOutButton className="w-full" />
          </PopoverPrimitive.Popup>
        </PopoverPrimitive.Positioner>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  )
}
