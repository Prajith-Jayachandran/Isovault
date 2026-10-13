CC ?= gcc
CFLAGS ?= -O2 -Wall
PKG_CFLAGS = $(shell pkg-config --cflags gtk+-3.0 json-glib-1.0)
PKG_LIBS = $(shell pkg-config --libs gtk+-3.0 json-glib-1.0)

all: isovault_gui

isovault_gui: isovault_gui.c
	$(CC) $(CFLAGS) $(PKG_CFLAGS) -o isovault_gui isovault_gui.c $(PKG_LIBS)

clean:
	rm -f isovault_gui

.PHONY: all clean
