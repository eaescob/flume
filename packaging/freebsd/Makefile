PORTNAME=	flume
DISTVERSIONPREFIX=	v
DISTVERSION=	1.2.5
CATEGORIES=	irc

MAINTAINER=	emilio@flumeirc.io
COMMENT=	Modern terminal IRC client with scripting and LLM support
WWW=		https://github.com/FlumeIRC/flume

LICENSE=	APACHE20
LICENSE_FILE=	${WRKSRC}/LICENSE

USES=		cargo python:3.7+
USE_GITHUB=	yes
GH_ACCOUNT=	FlumeIRC

.include "${.CURDIR}/Makefile.crates"

CARGO_BUILD_ARGS=	-p flume-tui --features python
CARGO_INSTALL_PATH=	flume-tui
CARGO_ENV=	PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1

PLIST_FILES=	bin/flume \
		share/man/man1/flume.1.gz

post-install:
	${INSTALL_MAN} ${WRKSRC}/doc/flume.1 ${STAGEDIR}${PREFIX}/share/man/man1

.include <bsd.port.mk>
