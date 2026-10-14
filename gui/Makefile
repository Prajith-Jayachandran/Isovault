CC ?= gcc
CFLAGS ?= -O2 -Wall
PKG_CFLAGS = $(shell pkg-config --cflags gtk+-3.0 json-glib-1.0)
PKG_LIBS = $(shell pkg-config --libs gtk+-3.0 json-glib-1.0)

all: isovault_gui

isovault_gui_glade.h: isovault_gui.glade
	xxd -i isovault_gui.glade > isovault_gui_glade.h 2>/dev/null || python3 -c 'd=open("isovault_gui.glade","rb").read(); print("unsigned char isovault_gui_glade[] = {" + ",".join(str(b) for b in d) + ", 0};\nunsigned int isovault_gui_glade_len = " + str(len(d)) + ";")' > isovault_gui_glade.h

isovault_gui: isovault_gui.c isovault_gui_glade.h
	$(CC) $(CFLAGS) $(PKG_CFLAGS) -o isovault_gui isovault_gui.c $(PKG_LIBS)

clean:
	rm -f isovault_gui

.PHONY: all clean
