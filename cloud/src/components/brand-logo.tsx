import { AppLogo, type AppLogoProps } from "@xero/ui/components/app-logo";

type BrandLogoProps = AppLogoProps;

export function BrandLogo(props: BrandLogoProps) {
	return <AppLogo {...props} />;
}
