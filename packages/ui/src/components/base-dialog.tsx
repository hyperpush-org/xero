'use client'

import * as React from 'react'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from './ui/alert-dialog'
import { Button } from './ui/button'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from './ui/dialog'

export type BaseDialogVariant =
  | 'alert'
  | 'confirmation'
  | 'custom'
  | 'destructive-confirmation'
  | 'editor'
  | 'form'
  | 'info'

type ButtonProps = React.ComponentProps<typeof Button>
type DialogRootProps = Omit<React.ComponentProps<typeof Dialog>, 'children'>
type DialogContentProps = Omit<
  React.ComponentProps<typeof DialogContent>,
  'children' | 'className'
>
type AlertDialogRootProps = Omit<
  React.ComponentProps<typeof AlertDialog>,
  'children'
>
type AlertDialogContentProps = Omit<
  React.ComponentProps<typeof AlertDialogContent>,
  'children' | 'className'
>

export interface BaseDialogAction {
  label: React.ReactNode
  onClick?: React.MouseEventHandler<HTMLButtonElement>
  type?: ButtonProps['type']
  variant?: ButtonProps['variant']
  size?: ButtonProps['size']
  className?: string
  disabled?: boolean
  loading?: boolean
  loadingLabel?: React.ReactNode
  destructive?: boolean
  autoFocus?: boolean
  close?: boolean
}

export interface BaseAlertDialogAction {
  label: React.ReactNode
  onClick?: React.MouseEventHandler<HTMLButtonElement>
  className?: string
  disabled?: boolean
  loading?: boolean
  loadingLabel?: React.ReactNode
  destructive?: boolean
}

export interface BaseDialogProps extends DialogRootProps {
  variant?: BaseDialogVariant
  title: React.ReactNode
  description?: React.ReactNode
  children?: React.ReactNode
  trigger?: React.ReactNode
  leading?: React.ReactNode
  header?: React.ReactNode
  footer?: React.ReactNode
  actions?: BaseDialogAction[]
  busy?: boolean
  contentClassName?: string
  contentProps?: DialogContentProps
  bodyClassName?: string
  headerClassName?: string
  titleClassName?: string
  descriptionClassName?: string
  footerClassName?: string
  overlayClassName?: string
  showCloseButton?: boolean
}

export interface BaseAlertDialogProps extends AlertDialogRootProps {
  variant?: Extract<
    BaseDialogVariant,
    'alert' | 'confirmation' | 'destructive-confirmation'
  >
  title: React.ReactNode
  description?: React.ReactNode
  children?: React.ReactNode
  trigger?: React.ReactNode
  header?: React.ReactNode
  footer?: React.ReactNode
  cancelAction?: BaseAlertDialogAction
  action?: BaseAlertDialogAction
  busy?: boolean
  contentClassName?: string
  contentProps?: AlertDialogContentProps
  headerClassName?: string
  titleClassName?: string
  descriptionClassName?: string
  footerClassName?: string
}

export function BaseDialog({
  variant = 'custom',
  title,
  description,
  children,
  trigger,
  leading,
  header,
  footer,
  actions,
  busy = false,
  contentClassName,
  contentProps,
  bodyClassName,
  headerClassName,
  titleClassName,
  descriptionClassName,
  footerClassName,
  overlayClassName,
  showCloseButton,
  ...rootProps
}: BaseDialogProps) {
  const hasFooter = Boolean(footer || actions?.length)

  return (
    <Dialog {...rootProps}>
      {trigger ? <DialogTrigger asChild>{trigger}</DialogTrigger> : null}
      <DialogContent
        className={contentClassName}
        overlayClassName={overlayClassName}
        showCloseButton={showCloseButton}
        data-dialog-variant={variant}
        {...contentProps}
      >
        {leading}
        {header === undefined ? (
          <DialogHeader className={headerClassName}>
            <DialogTitle className={titleClassName}>{title}</DialogTitle>
            {description ? (
              <DialogDescription className={descriptionClassName}>
                {description}
              </DialogDescription>
            ) : null}
          </DialogHeader>
        ) : (
          header
        )}
        {bodyClassName ? <div className={bodyClassName}>{children}</div> : children}
        {hasFooter ? (
          <DialogFooter className={footerClassName}>
            {footer ?? <BaseDialogActions actions={actions ?? []} busy={busy} />}
          </DialogFooter>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}

export function BaseAlertDialog({
  variant = 'confirmation',
  title,
  description,
  children,
  trigger,
  header,
  footer,
  cancelAction,
  action,
  busy = false,
  contentClassName,
  contentProps,
  headerClassName,
  titleClassName,
  descriptionClassName,
  footerClassName,
  ...rootProps
}: BaseAlertDialogProps) {
  const resolvedAction = action
    ? {
        ...action,
        destructive:
          action.destructive ?? variant === 'destructive-confirmation',
      }
    : undefined
  const hasFooter = Boolean(footer || cancelAction || resolvedAction)

  return (
    <AlertDialog {...rootProps}>
      {trigger ? <AlertDialogTrigger asChild>{trigger}</AlertDialogTrigger> : null}
      <AlertDialogContent
        className={contentClassName}
        data-dialog-variant={variant}
        {...contentProps}
      >
        {header === undefined ? (
          <AlertDialogHeader className={headerClassName}>
            <AlertDialogTitle className={titleClassName}>{title}</AlertDialogTitle>
            {description ? (
              <AlertDialogDescription className={descriptionClassName}>
                {description}
              </AlertDialogDescription>
            ) : null}
          </AlertDialogHeader>
        ) : (
          header
        )}
        {children}
        {hasFooter ? (
          <AlertDialogFooter className={footerClassName}>
            {footer ?? (
              <>
                {cancelAction ? (
                  <AlertDialogCancel
                    disabled={busy || cancelAction.disabled || cancelAction.loading}
                    className={cancelAction.className}
                    onClick={cancelAction.onClick}
                  >
                    {cancelAction.loading
                      ? cancelAction.loadingLabel ?? cancelAction.label
                      : cancelAction.label}
                  </AlertDialogCancel>
                ) : null}
                {resolvedAction ? (
                  <AlertDialogAction
                    disabled={busy || resolvedAction.disabled || resolvedAction.loading}
                    className={
                      resolvedAction.destructive
                        ? `bg-destructive text-destructive-foreground hover:bg-destructive/90 ${resolvedAction.className ?? ''}`
                        : resolvedAction.className
                    }
                    onClick={resolvedAction.onClick}
                  >
                    {resolvedAction.loading
                      ? resolvedAction.loadingLabel ?? resolvedAction.label
                      : resolvedAction.label}
                  </AlertDialogAction>
                ) : null}
              </>
            )}
          </AlertDialogFooter>
        ) : null}
      </AlertDialogContent>
    </AlertDialog>
  )
}

export function BaseDialogActions({
  actions,
  busy = false,
}: {
  actions: BaseDialogAction[]
  busy?: boolean
}) {
  return (
    <>
      {actions.map((action, index) => {
        const button = (
          <Button
            key={index}
            type={action.type ?? 'button'}
            variant={
              action.variant ?? (action.destructive ? 'destructive' : undefined)
            }
            size={action.size}
            className={action.className}
            disabled={busy || action.disabled || action.loading}
            autoFocus={action.autoFocus}
            onClick={action.onClick}
          >
            {action.loading ? action.loadingLabel ?? action.label : action.label}
          </Button>
        )

        return action.close ? (
          <DialogClose key={index} asChild>
            {button}
          </DialogClose>
        ) : (
          button
        )
      })}
    </>
  )
}
